//! Shared process + output-contract helpers. Every verb spawns children and emits its
//! `RESULT scope=… status=…` line through this module — never spawn ad hoc per verb.
//!
//! Cleanup-sensitive workflows capture and replay child output through
//! [`run_captured_with_env`]. There is no separate gate binary; process spawning remains
//! centralized here.
//!
//! `run_in` and `result_fail_step` are starter helpers the example verbs don't yet call;
//! the module-level `allow(dead_code)` keeps a freshly scaffolded crate warning-clean. Drop
//! the attribute once your real verbs use them (they will).
#![allow(dead_code)]

use std::{
    ffi::OsStr,
    io::{self, Write as _},
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, bail};

/// Run `program args…`, returning an error tagged with `label` if it exits non-zero.
pub fn run(label: &str, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        bail!("{label} failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

/// Run `program args…` in `dir`, returning an error tagged with `label` on non-zero exit.
pub fn run_in(label: &str, dir: &str, program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program).current_dir(dir).args(args).status()?;
    if !status.success() {
        bail!("{label} failed (exit {})", status.code().unwrap_or(-1));
    }
    Ok(())
}

/// Run `program args…` and capture stdout as UTF-8; error on non-zero exit.
pub fn capture(label: &str, program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program).args(args).output()?;
    if !out.status.success() {
        bail!("{label} failed (exit {})", out.status.code().unwrap_or(-1));
    }
    String::from_utf8(out.stdout).context("non-UTF-8 output")
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
pub fn result(scope: &str, status: &str) {
    println!("RESULT scope={scope} status={status}");
}

/// Emit a FAIL contract line naming the failed step.
pub fn result_fail_step(scope: &str, step: &str) {
    println!("RESULT scope={scope} status=FAIL step={step}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ok_on_true() {
        assert!(run("noop", "true", &[]).is_ok());
    }

    #[test]
    fn run_err_on_false() {
        let err = run("boom", "false", &[]).unwrap_err();
        assert!(err.to_string().contains("boom failed"));
    }

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
}
