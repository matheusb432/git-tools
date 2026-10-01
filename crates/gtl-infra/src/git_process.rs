//! Private fallback for Git operations not covered by the adopted `gix` facade.

use std::{path::Path, process::Command};

use anyhow::Context;
#[derive(Debug)]
pub(crate) struct GitProcessOutput {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) exit_code: i32,
}

impl GitProcessOutput {
    pub(crate) fn success(&self) -> bool {
        self.exit_code == 0
    }

    pub(crate) fn diagnostic(&self) -> &str {
        let stderr = self.stderr.trim();
        if stderr.is_empty() {
            self.stdout.trim()
        } else {
            stderr
        }
    }

    pub(crate) fn error_line(&self) -> String {
        last_non_empty_line(self.diagnostic()).map_or_else(
            || format!("git command failed (exit {})", self.exit_code),
            str::to_string,
        )
    }

    pub(crate) fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

pub(crate) fn run(repo_path: &Path, args: &[&str]) -> anyhow::Result<GitProcessOutput> {
    run_with_index(repo_path, args, None)
}

pub(crate) fn command() -> Command {
    let mut command = Command::new("git");
    command.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub(crate) fn run_with_index(
    repo_path: &Path,
    args: &[&str],
    index: Option<&Path>,
) -> anyhow::Result<GitProcessOutput> {
    let mut command = command();
    command.arg("-C").arg(repo_path).args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    if let Some(cancellation) = crate::git_client::status_context::cancellation() {
        return run_status_command(command, &cancellation);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run git in {}", repo_path.display()))?;

    Ok(GitProcessOutput {
        stdout: String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")?,
        stderr: String::from_utf8_lossy_owned(output.stderr),
        exit_code: output.status.code().unwrap_or(1),
    })
}

fn run_status_command(
    command: Command,
    cancellation: &std::sync::atomic::AtomicBool,
) -> anyhow::Result<GitProcessOutput> {
    run_timed_command(command, cancellation, std::time::Duration::from_secs(10))
}

pub(crate) fn run_bounded(
    repo: &Path,
    args: &[&str],
    timeout: std::time::Duration,
) -> anyhow::Result<GitProcessOutput> {
    let mut command = command();
    command
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    run_timed_command(command, &std::sync::atomic::AtomicBool::new(false), timeout)
}

fn run_timed_command(
    mut command: Command,
    cancellation: &std::sync::atomic::AtomicBool,
    timeout: std::time::Duration,
) -> anyhow::Result<GitProcessOutput> {
    use std::{
        io::{Read as _, Seek as _},
        process::Stdio,
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };
    anyhow::ensure!(!cancellation.load(Ordering::Relaxed), "status cancelled");
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = command
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?))
        .spawn()?;
    let started = Instant::now();
    let exit = loop {
        if cancellation.load(Ordering::Relaxed)
            || started.elapsed() > timeout
            || stdout
                .metadata()
                .map_or(true, |metadata| metadata.len() > 128 * 1024)
            || stderr
                .metadata()
                .map_or(true, |metadata| metadata.len() > 128 * 1024)
        {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "Git command cancelled, timed out, or exceeded its output limit; check repository status before retrying"
            );
        }
        match child.try_wait() {
            Ok(Some(exit)) => break exit,
            Ok(None) => std::thread::sleep(Duration::from_millis(2)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
    };
    let read = |file: &mut std::fs::File| -> anyhow::Result<String> {
        anyhow::ensure!(
            file.metadata()?.len() <= 128 * 1024,
            "status command output exceeds limit"
        );
        file.rewind()?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        Ok(text)
    };
    Ok(GitProcessOutput {
        stdout: read(&mut stdout)?,
        stderr: read(&mut stderr)?,
        exit_code: exit.code().unwrap_or(1),
    })
}

fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}
