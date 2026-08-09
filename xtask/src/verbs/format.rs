//! Formatting verbs.
//!
//! `run`/`check` drive the repository's complete formatter matrix (pinned-nightly rustfmt, Taplo,
//! Dioxus RSX, rumdl, and the Deno frontend formatter) as a [`Step`] plan. The aggregate
//! read-only gate (`check`) and `fix` reuse the same plan through `check_steps` / `write_steps`;
//! the linters live in the sibling `lint` module.

use anyhow::Result;
use clap::Args;

use crate::{
    process::{self, Status},
    task::{self, Step},
    verb::Verb,
};

mod dioxus;
pub(crate) mod markdown;
mod rust;

/// `fmt` / `fmt-check` flags.
#[derive(Args)]
pub(crate) struct FormatArguments {
    /// Surface taplo's file-discovery logs in its verbose format instead of suppressing them.
    #[arg(long)]
    pub(crate) verbose: bool,
}

/// Whether a formatter step writes changes or only verifies them.
#[derive(Clone, Copy)]
pub(super) enum FormatMode {
    Write,
    Check,
}

impl FormatMode {
    fn check_argument(self) -> Option<&'static str> {
        match self {
            Self::Write => None,
            Self::Check => Some("--check"),
        }
    }
}

/// Apply every configured formatter in place.
pub(crate) fn run(verbose: bool) -> Result<()> {
    task::run_all(&write_steps(verbose)?)?;
    process::result(Verb::FORMAT, Status::Done);
    Ok(())
}

/// Verify formatting without modifying files (exits non-zero on drift).
pub(crate) fn check(verbose: bool) -> Result<()> {
    task::check_all(&check_steps(verbose)?, "run `just fmt`")?;
    check_dioxus()?;
    process::result(Verb::FORMAT_CHECK, Status::Pass);
    Ok(())
}

pub(crate) fn check_dioxus() -> Result<()> {
    dioxus::check()
}

pub(crate) fn dioxus_check_step(directory: &std::path::Path) -> Step {
    dioxus::check_step(directory)
}

/// The write-mode formatter plan, shared with `fix`.
pub(super) fn write_steps(verbose: bool) -> Result<Vec<Step>> {
    format_steps(FormatMode::Write, verbose)
}

/// The check-mode formatter plan, shared with the aggregate `check` gate.
pub(super) fn check_steps(verbose: bool) -> Result<Vec<Step>> {
    format_steps(FormatMode::Check, verbose)
}

/// The complete formatter matrix for `mode`.
fn format_steps(mode: FormatMode, verbose: bool) -> Result<Vec<Step>> {
    let mut steps = vec![rust::format_step(mode)?, taplo_step(mode, verbose)];
    if matches!(mode, FormatMode::Write) {
        steps.push(dioxus::format_step());
    }
    steps.extend(markdown::format_step(mode)?);
    steps.push(frontend_step(mode));
    Ok(steps)
}

/// `taplo fmt [--check]`. Without `verbose`, `RUST_LOG=warn` suppresses taplo's INFO
/// file-discovery lines; with it, taplo's own `--verbose` flag restores the richer detail.
fn taplo_step(mode: FormatMode, verbose: bool) -> Step {
    let step = Step::new("taplo", "taplo", ["fmt"]).with_arguments(mode.check_argument());
    if verbose {
        step.with_arguments(["--verbose"])
    } else {
        step.with_environment("RUST_LOG", "warn")
    }
}

/// `deno task --frozen format[:check]` — the framework-free frontend formatter (Oxfmt).
fn frontend_step(mode: FormatMode) -> Step {
    let task = match mode {
        FormatMode::Write => "format",
        FormatMode::Check => "format:check",
    };
    Step::new("frontend-format", "deno", ["task", "--frozen", task])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argument_strings(step: &Step) -> Vec<&str> {
        step.arguments().iter().map(String::as_str).collect()
    }

    #[test]
    fn taplo_toggles_check_on_verify() {
        assert_eq!(
            argument_strings(&taplo_step(FormatMode::Write, false)),
            ["fmt"]
        );
        assert_eq!(
            argument_strings(&taplo_step(FormatMode::Check, false)),
            ["fmt", "--check"]
        );
    }

    #[test]
    fn quiet_taplo_suppresses_logs_without_verbose_flag() {
        let step = taplo_step(FormatMode::Write, false);
        assert_eq!(
            step.environment(),
            [("RUST_LOG".to_string(), "warn".to_string())]
        );
        assert!(!argument_strings(&step).contains(&"--verbose"));
    }

    #[test]
    fn verbose_taplo_forwards_flag_without_env() {
        let step = taplo_step(FormatMode::Write, true);
        assert!(argument_strings(&step).contains(&"--verbose"));
        assert!(step.environment().is_empty());
    }

    #[test]
    fn frontend_task_matches_mode() {
        assert_eq!(
            argument_strings(&frontend_step(FormatMode::Write)),
            ["task", "--frozen", "format"]
        );
        assert_eq!(
            argument_strings(&frontend_step(FormatMode::Check)),
            ["task", "--frozen", "format:check"]
        );
    }
}
