use std::{ffi::OsString, time::Duration};

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use sample_project::{OutputPath, Run, Test, TestCountDiscovery, surface};

use crate::{process, task::Step};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArguments {
    #[command(subcommand)]
    command: Option<TestCommand>,
    /// Stream full test output live; the log still captures it.
    #[arg(long)]
    pub(crate) verbose: bool,
    /// Emit one JSON report to stdout.
    #[arg(long)]
    pub(crate) json: bool,
    /// Provide an evidence directory to E2E tests.
    #[arg(long)]
    pub(crate) evidences: bool,
    #[command(flatten)]
    selection: TestSelectionArguments,
}

#[derive(Args)]
struct TestSelectionArguments {
    /// Test scope.
    #[arg(
        long,
        value_enum,
        default_value_t = Scope::Unit,
        default_value_ifs = [("e2e", "true", "e2e"), ("all", "true", "all")]
    )]
    pub(crate) scope: Scope,
    /// Shorthand for `--scope e2e`.
    #[arg(long, conflicts_with_all = ["all", "scope"])]
    e2e: bool,
    /// Shorthand for `--scope all`.
    #[arg(long, conflicts_with = "scope")]
    all: bool,
}

#[derive(Subcommand)]
enum TestCommand {
    /// Collect workspace test coverage with cargo-llvm-cov.
    Coverage(TestCoverageArguments),
}

#[derive(Args)]
#[command(disable_help_flag = true)]
struct TestCoverageArguments {
    /// Extra cargo-llvm-cov arguments; output defaults to --quiet when unspecified.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    arguments_extra: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Scope {
    Unit,
    E2e,
    All,
}

struct TestDeclaration {
    #[cfg(test)]
    label: &'static str,
    test: Test,
}

impl TestDeclaration {
    fn new(label: &'static str, test: Test) -> Self {
        #[cfg(not(test))]
        let _ = label;
        Self {
            #[cfg(test)]
            label,
            test,
        }
    }

    #[cfg(test)]
    fn label(&self) -> &'static str {
        self.label
    }
}

pub(crate) fn run(arguments: &TestArguments) -> Result<()> {
    if let Some(TestCommand::Coverage(coverage)) = &arguments.command {
        return test_coverage(&coverage.arguments_extra);
    }

    let executable = std::env::current_exe().context("resolve the xtask executable")?;
    let scope = arguments.selection.scope;
    let declarations = selected_tests(scope, executable.into_os_string())?;
    let tests = declarations.into_iter().map(|declaration| declaration.test);

    Run::try_new(scope.to_string(), tests)?
        .verbose(arguments.verbose)
        .json(arguments.json)
        .output_path(OutputPath::default())
        .try_with_evidences(arguments.evidences)?
        .execute()?;

    Ok(())
}

fn test_coverage(arguments_extra: &[String]) -> Result<()> {
    process::run_step(&test_coverage_step(arguments_extra))?;
    if coverage_cleanup_is_required(arguments_extra) {
        process::run_step(&Step::new(
            "clean coverage artifacts",
            "cargo",
            ["clean", "--target-dir", "target/llvm-cov-target"],
        ))?;
    }
    Ok(())
}

fn test_coverage_step(arguments_extra: &[String]) -> Step {
    let mut arguments = vec!["llvm-cov".to_string()];
    if !cargo_package_scope_is_explicit(arguments_extra) {
        arguments.push("--workspace".to_string());
    }
    if !coverage_output_is_explicit(arguments_extra) {
        arguments.push("--quiet".to_string());
    }
    Step::new("test coverage", "cargo", arguments).with_arguments(arguments_extra.iter().cloned())
}

fn cargo_package_scope_is_explicit(arguments: &[String]) -> bool {
    arguments
        .iter()
        .take_while(|argument| argument.as_str() != "--")
        .any(|argument| {
            matches!(
                argument.as_str(),
                "-p" | "--package" | "--workspace" | "--all" | "--manifest-path"
            ) || argument.starts_with("-p=")
                || argument.starts_with("--package=")
                || argument.starts_with("--manifest-path=")
        })
}

fn coverage_cleanup_is_required(arguments: &[String]) -> bool {
    !arguments
        .iter()
        .take_while(|argument| argument.as_str() != "--")
        .any(|argument| matches!(argument.as_str(), "-h" | "--help" | "--no-report"))
}

fn coverage_output_is_explicit(arguments: &[String]) -> bool {
    arguments
        .iter()
        .take_while(|argument| argument.as_str() != "--")
        .any(|argument| {
            matches!(argument.as_str(), "--quiet" | "--verbose")
                || argument.strip_prefix('-').is_some_and(|flags| {
                    !flags.is_empty() && flags.bytes().all(|flag| matches!(flag, b'q' | b'v'))
                })
        })
}

fn selected_tests(scope: Scope, executable: OsString) -> Result<Vec<TestDeclaration>> {
    match scope {
        Scope::Unit => tests_unit(),
        Scope::E2e => tests_e2e(executable),
        Scope::All => tests_all(executable),
    }
}

fn tests_unit() -> Result<Vec<TestDeclaration>> {
    Ok(vec![
        TestDeclaration::new(
            "unit",
            Test::try_new("unit", surface::CARGO, "cargo")?
                .args(["test", "--quiet"])
                .test_count_discovery(TestCountDiscovery::CARGO_TEST_HARNESS)
                .verbose_arguments(["--", "--nocapture"]),
        ),
        parser_all_features()?,
        web_desktop()?,
        web_artifact()?,
    ])
}

fn tests_e2e(executable: OsString) -> Result<Vec<TestDeclaration>> {
    Ok(vec![desktop_e2e(executable)?])
}

fn tests_all(executable: OsString) -> Result<Vec<TestDeclaration>> {
    Ok(vec![
        TestDeclaration::new(
            "unit",
            Test::try_new("unit", surface::CARGO, "cargo")?
                .args(["test", "--workspace", "--quiet"])
                .test_count_discovery(TestCountDiscovery::CARGO_TEST_HARNESS)
                .verbose_arguments(["--", "--nocapture"]),
        ),
        parser_all_features()?,
        web_artifact()?,
        worker("drift", &executable, "drift-check")?,
        desktop_e2e(executable)?,
    ])
}

fn parser_all_features() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "parser-all-features",
        Test::try_new("parser-all-features", surface::CARGO, "cargo")?
            .args([
                "test",
                "--locked",
                "--quiet",
                "-p",
                "gtl-parser",
                "--all-features",
            ])
            .test_count_discovery(TestCountDiscovery::CARGO_TEST_HARNESS),
    ))
}

fn web_desktop() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "web-desktop",
        Test::try_new("web-desktop", surface::CARGO, "cargo")?
            .args(["test", "--locked", "--quiet", "-p", "gtl-web"])
            .test_count_discovery(TestCountDiscovery::CARGO_TEST_HARNESS),
    ))
}

fn web_artifact() -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "web-artifact",
        Test::try_new("web-artifact", surface::CARGO, "cargo")?
            .args([
                "test",
                "--locked",
                "--quiet",
                "-p",
                "gtl-web",
                "--no-default-features",
                "--features",
                "artifact",
            ])
            .test_count_discovery(TestCountDiscovery::CARGO_TEST_HARNESS),
    ))
}

fn worker(
    label: &'static str,
    executable: &OsString,
    verb: &'static str,
) -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        label,
        Test::try_new(label, surface::OPAQUE, executable.clone())?.arg(verb),
    ))
}

fn desktop_e2e(executable: OsString) -> Result<TestDeclaration> {
    Ok(TestDeclaration::new(
        "desktop-e2e",
        Test::try_new("desktop-e2e", surface::OPAQUE, executable)?
            .arg("desktop-e2e-worker")
            .verbose_arguments(["--verbose"])
            .accepts_evidences()
            .timeout(Duration::from_hours(2)),
    ))
}

impl std::fmt::Display for Scope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unit => "unit",
            Self::E2e => "e2e",
            Self::All => "all",
        })
    }
}

#[cfg(test)]
fn selected_test_labels(scope: Scope) -> Vec<&'static str> {
    selected_tests(scope, "xtask".into())
        .expect("static test declarations are valid")
        .iter()
        .map(TestDeclaration::label)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_selects_default_member_tests() {
        assert_eq!(
            selected_test_labels(Scope::Unit),
            ["unit", "parser-all-features", "web-desktop", "web-artifact"]
        );
    }

    #[test]
    fn e2e_selects_the_viewer_journeys() {
        assert_eq!(selected_test_labels(Scope::E2e), ["desktop-e2e"]);
    }

    #[test]
    fn all_selects_the_complete_test_suite_in_declaration_order() {
        assert_eq!(
            selected_test_labels(Scope::All),
            [
                "unit",
                "parser-all-features",
                "web-artifact",
                "drift",
                "desktop-e2e"
            ]
        );
    }

    #[test]
    fn test_coverage_defaults_to_workspace_and_forwards_arguments() {
        let step = test_coverage_step(&["--show-missing-lines".to_string()]);

        assert_eq!(step.label(), "test coverage");
        assert_eq!(step.program(), "cargo");
        assert_eq!(
            step.arguments(),
            ["llvm-cov", "--workspace", "--quiet", "--show-missing-lines"]
        );
    }

    #[test]
    fn test_coverage_preserves_a_package_scope() {
        let step = test_coverage_step(&["--package=xtask".to_string()]);

        assert_eq!(step.arguments(), ["llvm-cov", "--quiet", "--package=xtask"]);
    }

    #[test]
    fn test_coverage_preserves_explicit_output_options_before_test_arguments() {
        for option in ["-q", "-v", "-vv", "--quiet", "--verbose"] {
            assert!(coverage_output_is_explicit(&[option.to_string()]));
        }
        assert!(!coverage_output_is_explicit(&[
            "--".to_string(),
            "--verbose".to_string(),
        ]));
    }

    #[test]
    fn test_coverage_cleanup_requires_a_generated_report() {
        assert!(!coverage_cleanup_is_required(&["--help".to_string()]));
        assert!(!coverage_cleanup_is_required(&["--no-report".to_string()]));
        assert!(coverage_cleanup_is_required(&[
            "--".to_string(),
            "--help".to_string(),
        ]));
    }
}
