//! Typed test-scope orchestration. The `unit` scope intentionally uses workspace default-members
//! so desktop Rust tests remain opt-in behind `all`; `e2e` runs only the hermetic viewer suite.

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::process;

/// `test` flags. `--e2e` / `--all` are shorthands that resolve `--scope` through
/// `default_value_ifs`.
#[derive(Args)]
pub(crate) struct TestArguments {
    /// Stream full test output (`cargo test -- --nocapture`) instead of the terse default.
    #[arg(long)]
    pub(crate) verbose: bool,
    /// Test scope: `unit` (default; desktop excluded), `e2e` (hermetic viewer only), or `all`.
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

/// Which part of the suite runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Scope {
    Unit,
    E2e,
    All,
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

fn cargo_test_command(verbose: bool, workspace: bool) -> String {
    let workspace = if workspace { " --workspace" } else { "" };
    if verbose {
        format!("cargo test{workspace} -- --nocapture")
    } else {
        format!("cargo test{workspace} --quiet")
    }
}

/// Run the selected test scope.
pub(crate) fn run(scope: Scope, verbose: bool) -> Result<()> {
    match scope {
        Scope::E2e => super::desktop_e2e::run(),
        Scope::Unit => {
            process::gate("check", "just check", verbose)?;
            process::gate("rust_tests", &cargo_test_command(verbose, false), verbose)
        }
        Scope::All => {
            process::gate("check", "just check", verbose)?;
            process::gate(
                "rust_tests_all",
                &cargo_test_command(verbose, true),
                verbose,
            )?;
            process::gate("frontend_tests", "just cli test", verbose)?;
            process::gate("bundle_drift", "just drift-check", verbose)?;
            super::desktop_e2e::run()
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
}
