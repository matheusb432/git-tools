//! Shared process execution and the `RESULT scope=… status=…` output contract. Every verb spawns
//! children and emits its result line through this module — never spawn ad hoc per verb.
//!
//! Command plans expressed as [`crate::task::Step`] run through [`run_step`] and [`step_succeeds`];
//! cleanup-sensitive workflows capture and replay child output through [`run_captured_with_env`].

use std::{
    ffi::OsStr,
    io::{self, Write as _},
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{task::Step, verb::Verb};

/// A terminal verb outcome for the `RESULT` line: `PASS` for a read-only gate, `DONE` for a
/// mutation.
#[derive(Clone, Copy)]
pub(crate) enum Status {
    Pass,
    Done,
}

impl std::fmt::Display for Status {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Pass => "PASS",
            Self::Done => "DONE",
        })
    }
}

/// Run one command plan, tagging any non-zero exit with the step's label.
pub(crate) fn run_step(step: &Step) -> Result<()> {
    let status = step_command(step).status()?;
    if !status.success() {
        bail!(
            "{} failed (exit {})",
            step.label(),
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

/// Run one command plan and report whether it succeeded (used by the read-only gate).
pub(crate) fn step_succeeds(step: &Step) -> Result<bool> {
    let status = step_command(step)
        .status()
        .with_context(|| format!("spawning {}", step.label()))?;
    Ok(status.success())
}

/// Build the child command for a step, applying its arguments and environment additions.
fn step_command(step: &Step) -> Command {
    let mut command = Command::new(step.program());
    command
        .args(step.arguments())
        .envs(step.environment().iter().map(|(key, value)| (key, value)));
    for name in step.removed_environment() {
        command.env_remove(name);
    }
    if let Some(directory) = step.current_directory() {
        command.current_dir(directory);
    }
    command
}

/// Run `program args…`, returning an error tagged with `label` if it exits non-zero.
pub fn run(label: &str, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        bail!("{label} failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

/// Run `program args…` and capture stdout as UTF-8; error on non-zero exit.
pub fn capture(label: &str, program: &str, args: &[&str]) -> Result<String> {
    String::from_utf8(capture_bytes(label, program, args)?).context("non-UTF-8 output")
}

pub(crate) fn capture_bytes(label: &str, program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        bail!(
            "{label} failed (exit {})",
            output.status.code().unwrap_or(-1)
        );
    }
    Ok(output.stdout)
}

/// Run a command with an optional working directory and explicit environment additions.
///
/// Captures the child so callers can sequence cleanup deterministically, then replays both
/// streams unchanged before reporting a spawn or exit failure tagged with `label`.
pub fn run_captured_with_env(
    label: &str,
    dir: Option<&Path>,
    program: &OsStr,
    args: &[&str],
    env: &[(&str, &OsStr)],
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).envs(env.iter().copied());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output().with_context(|| format!("start {label}"))?;
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    if !output.status.success() {
        bail!(
            "{label} failed (exit {})",
            output.status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

/// Emit the parsed contract line on stdout (keep this byte-stable — consumers grep it).
pub(crate) fn result(scope: Verb, status: Status) {
    println!("RESULT scope={scope} status={status}");
}

/// Emit a FAIL contract line naming the failed step.
pub(crate) fn result_fail_step(scope: Verb, step: &str) {
    println!("RESULT scope={scope} status=FAIL step={step}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_helper_propagates_environment_and_working_directory() {
        let executable = std::env::current_exe().expect("test executable resolves");
        let directory = tempfile::tempdir().expect("temporary directory");
        let env = [("XTASK_PROC_EXPECTED_CWD", directory.path().as_os_str())];

        run_captured_with_env(
            "captured child",
            Some(directory.path()),
            executable.as_os_str(),
            &[
                "--exact",
                "proc::tests::captured_helper_child_observes_environment",
            ],
            &env,
        )
        .expect("captured child sees command configuration");
    }

    #[test]
    fn captured_helper_child_observes_environment() {
        let Some(expected) = std::env::var_os("XTASK_PROC_EXPECTED_CWD") else {
            return;
        };
        assert_eq!(
            std::env::current_dir().expect("current directory"),
            Path::new(&expected)
        );
    }

    #[test]
    fn step_command_removes_environment_before_spawn() {
        let executable = std::env::current_exe().expect("test executable resolves");
        let step = Step::new(
            "removed environment child",
            executable.to_string_lossy().into_owned(),
            [
                "--exact",
                "proc::tests::removed_environment_child_does_not_observe_variable",
            ],
        )
        .with_environment("XTASK_PROC_REMOVED", "present")
        .without_environment(["XTASK_PROC_REMOVED"]);

        assert!(
            step_command(&step)
                .status()
                .expect("child starts")
                .success()
        );
    }

    #[test]
    fn removed_environment_child_does_not_observe_variable() {
        assert!(std::env::var_os("XTASK_PROC_REMOVED").is_none());
    }
}
