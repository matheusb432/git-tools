//! `StdGitRunner`: the [`GitRunner`] adapter that shells out to the real `git`
//! binary and captures its output verbatim.

use std::{path::Path, process::Command};

use anyhow::Context;
use application::ports::{GitOutput, GitRunner};

/// Runs `git -C <repo> <args…>` as a subprocess.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdGitRunner;

impl GitRunner for StdGitRunner {
    fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .with_context(|| format!("failed to run git in {}", repo.display()))?;

        Ok(GitOutput {
            stdout: String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")?,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(1),
        })
    }
}
