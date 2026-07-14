//! Typed test-scope orchestration. Default tests intentionally use workspace default-members so
//! desktop Rust tests remain opt-in behind `--all`; `--e2e` runs only the hermetic viewer suite.

use anyhow::Result;

use crate::proc;

/// Parsed test selection after clap has rejected conflicting flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestOptions {
    pub verbose: bool,
    pub e2e: bool,
    pub all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestScope {
    Default,
    E2e,
    All,
}

impl TestOptions {
    fn scope(self) -> TestScope {
        if self.e2e {
            TestScope::E2e
        } else if self.all {
            TestScope::All
        } else {
            TestScope::Default
        }
    }
}

fn cargo_test_command(verbose: bool, workspace: bool) -> String {
    let workspace = if workspace { " --workspace" } else { "" };
    if verbose {
        format!("cargo test{workspace} -- --nocapture")
    } else {
        format!("cargo test{workspace} --quiet")
    }
}

/// Run the selected test scope.
pub fn run(options: TestOptions) -> Result<()> {
    match options.scope() {
        TestScope::E2e => crate::desktop_e2e::run(),
        TestScope::Default => {
            proc::gate("fmt_check", "just fmt-check", options.verbose)?;
            proc::gate(
                "rust_tests",
                &cargo_test_command(options.verbose, false),
                options.verbose,
            )
        }
        TestScope::All => {
            proc::gate("fmt_check", "just fmt-check", options.verbose)?;
            proc::gate(
                "rust_tests_all",
                &cargo_test_command(options.verbose, true),
                options.verbose,
            )?;
            proc::gate("frontend_tests", "just cli test", options.verbose)?;
            proc::gate("bundle_drift", "just drift-check", options.verbose)?;
            crate::desktop_e2e::run()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_test_is_quiet_and_uses_default_members_by_default() {
        assert_eq!(cargo_test_command(false, false), "cargo test --quiet");
    }

    #[test]
    fn full_cargo_test_selects_the_workspace_and_streams_when_verbose() {
        assert_eq!(
            cargo_test_command(true, true),
            "cargo test --workspace -- --nocapture"
        );
    }

    #[test]
    fn desktop_tests_are_reachable_only_from_explicit_scopes() {
        assert_eq!(
            TestOptions {
                verbose: false,
                e2e: false,
                all: false
            }
            .scope(),
            TestScope::Default
        );
        assert_eq!(
            TestOptions {
                verbose: false,
                e2e: true,
                all: false
            }
            .scope(),
            TestScope::E2e
        );
        assert_eq!(
            TestOptions {
                verbose: false,
                e2e: false,
                all: true
            }
            .scope(),
            TestScope::All
        );
    }
}
