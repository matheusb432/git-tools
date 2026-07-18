//! Formatting verbs.
//!
//! `run`/`check` drive the repository's complete formatter matrix (pinned-nightly rustfmt, Taplo,
//! mdformat, and the Deno frontend formatter) as a [`Step`] plan. The aggregate read-only gate
//! (`check`) and `fix` reuse the same plan through `check_steps` / `write_steps`; the linters live
//! in the sibling `lint` module.

use anyhow::{Context, Result};

use crate::{
    process::{self, Status},
    task::{self, Step},
    verb::Verb,
};

mod markdown;
mod rust;

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
pub(crate) fn run() -> Result<()> {
    task::run_all(&write_steps()?)?;
    process::result(Verb::FORMAT, Status::Done);
    Ok(())
}

/// Verify formatting without modifying files (exits non-zero on drift).
pub(crate) fn check() -> Result<()> {
    task::check_all(&check_steps()?, "run `just fmt`")?;
    process::result(Verb::FORMAT_CHECK, Status::Pass);
    Ok(())
}

/// The write-mode formatter plan, shared with `fix`.
pub(super) fn write_steps() -> Result<Vec<Step>> {
    format_steps(FormatMode::Write)
}

/// The check-mode formatter plan, shared with the aggregate `check` gate.
pub(super) fn check_steps() -> Result<Vec<Step>> {
    format_steps(FormatMode::Check)
}

/// The complete formatter matrix for `mode`.
fn format_steps(mode: FormatMode) -> Result<Vec<Step>> {
    which::which("taplo").context(
        "required formatter `taplo` is missing; install taplo-cli through the declarative host configuration",
    )?;
    let mut steps = vec![rust::format_step(mode)?, taplo_step(mode)];
    steps.extend(markdown::format_step(mode)?);
    steps.push(frontend_step(mode));
    Ok(steps)
}

/// `taplo fmt [--check]`.
fn taplo_step(mode: FormatMode) -> Step {
    Step::new("taplo", "taplo", ["fmt"]).with_arguments(mode.check_argument())
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
        assert_eq!(argument_strings(&taplo_step(FormatMode::Write)), ["fmt"]);
        assert_eq!(
            argument_strings(&taplo_step(FormatMode::Check)),
            ["fmt", "--check"]
        );
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
