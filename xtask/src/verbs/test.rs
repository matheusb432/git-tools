//! Test runner.

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use sample_project::Run;

use crate::project;

#[expect(
    clippy::struct_excessive_bools,
    reason = "CLI flags map directly to Clap arguments"
)]
#[derive(Args)]
pub(crate) struct TestArguments {
    /// Stream full test output live; the log still captures it.
    #[arg(long)]
    pub(crate) verbose: bool,
    /// Emit one JSON report to stdout.
    #[arg(long)]
    pub(crate) json: bool,
    /// Provide an evidence directory to E2E tests and save report.json.
    #[arg(long)]
    pub(crate) evidences: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Scope {
    Unit,
    E2e,
    All,
}

pub(crate) fn run(arguments: &TestArguments) -> Result<()> {
    let executable = std::env::current_exe().context("resolve the xtask executable")?;
    let declarations = selected_tests(arguments.scope, executable.into_os_string());
    let tests = declarations
        .into_iter()
        .map(project::TestDeclaration::into_test);

    Run::new(arguments.scope.to_string(), tests)
        .verbose(arguments.verbose)
        .json(arguments.json)
        .evidences_from_cargo_manifest(arguments.evidences, include_str!("../../Cargo.toml"))?
        .execute()?;

    Ok(())
}

fn selected_tests(scope: Scope, executable: std::ffi::OsString) -> Vec<project::TestDeclaration> {
    match scope {
        Scope::Unit => project::tests_unit(&executable),
        Scope::E2e => project::tests_e2e(executable),
        Scope::All => project::tests_all(executable),
    }
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
        .iter()
        .map(project::TestDeclaration::label)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_selects_check_and_default_member_tests() {
        assert_eq!(selected_test_labels(Scope::Unit), ["check", "unit"]);
    }

    #[test]
    fn e2e_selects_only_the_desktop_worker() {
        assert_eq!(selected_test_labels(Scope::E2e), ["e2e"]);
    }

    #[test]
    fn all_selects_the_complete_gate_in_declaration_order() {
        assert_eq!(
            selected_test_labels(Scope::All),
            ["check", "unit", "web", "drift", "e2e"]
        );
    }
}
