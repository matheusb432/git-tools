//! Shared process execution and the `RESULT scope=… status=…` output contract. Every verb spawns
//! children and emits its result line through this module — never spawn ad hoc per verb.
//!
//! Command plans expressed as [`crate::task::Step`] run through [`run_step`]; cleanup-sensitive
//! workflows capture and replay child output through [`run_captured_with_env`].

use std::{
    ffi::OsStr,
    io::{self, Write as _},
    path::Path,
    process::{Child, Command, Stdio},
};

use anyhow::{Context, Result, anyhow, bail};

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

/// Spawn one command plan without changing its terminal process group.
pub(crate) fn spawn_step(step: &Step) -> Result<Child> {
    step_command(step)
        .spawn()
        .with_context(|| format!("spawning {}", step.label()))
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

pub(crate) fn capture_bytes_with_stdin(
    label: &str,
    directory: Option<&Path>,
    program: &str,
    args: &[&str],
    input: &[u8],
) -> Result<Vec<u8>> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }

    let mut child = command.spawn().with_context(|| format!("start {label}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("{label} did not expose stdin"))?;
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child
        .wait_with_output()
        .with_context(|| format!("wait for {label}"))?;
    writer
        .join()
        .map_err(|_| anyhow!("{label} stdin writer panicked"))?
        .with_context(|| format!("write input to {label}"))?;
    if !output.status.success() {
        bail!(
            "{label} failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
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
    use std::io::Read as _;

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

    #[cfg(target_os = "linux")]
    #[test]
    fn development_child_remains_in_the_callers_process_group() {
        const CHILD_MARKER: &str = "XTASK_PROC_GROUP_CHILD";
        if std::env::var_os(CHILD_MARKER).is_some() {
            std::thread::sleep(std::time::Duration::from_secs(10));
            return;
        }

        let executable = std::env::current_exe().expect("test executable resolves");
        let step = Step::new(
            "process group child",
            executable.to_string_lossy().into_owned(),
            [
                "--exact",
                "process::tests::development_child_remains_in_the_callers_process_group",
            ],
        )
        .with_environment(CHILD_MARKER, "1");
        let parent_group = linux_process_group(std::process::id()).expect("parent process group");
        let mut child = spawn_step(&step).expect("development child starts");
        let child_group = linux_process_group(child.id());
        child.kill().expect("development child stops");
        child.wait().expect("development child is reaped");

        assert_eq!(child_group.expect("child process group"), parent_group);
    }

    #[cfg(target_os = "linux")]
    fn linux_process_group(process_id: u32) -> std::io::Result<u32> {
        let stat = std::fs::read_to_string(format!("/proc/{process_id}/stat"))?;
        let (_, fields) = stat.rsplit_once(") ").ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "process stat has no command",
            )
        })?;
        let group = fields.split_whitespace().nth(2).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "process stat has no process group",
            )
        })?;
        group
            .parse()
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    #[test]
    fn removed_environment_child_does_not_observe_variable() {
        assert!(std::env::var_os("XTASK_PROC_REMOVED").is_none());
    }

    #[test]
    fn captured_stdin_helper_propagates_input() {
        let executable = std::env::current_exe().expect("test executable resolves");
        let _output = capture_bytes_with_stdin(
            "captured stdin child",
            None,
            executable.to_str().expect("test executable path is UTF-8"),
            &[
                "--exact",
                "proc::tests::captured_stdin_child_observes_input",
            ],
            b"expected input",
        )
        .expect("captured child returns output");
    }

    #[test]
    fn captured_stdin_child_observes_input() {
        if std::env::args().any(|argument| argument == "--exact") {
            let mut input = Vec::new();
            io::stdin()
                .read_to_end(&mut input)
                .expect("read captured input");
            assert_eq!(input, b"expected input");
        }
    }
}
