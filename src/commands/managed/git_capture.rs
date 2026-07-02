//! Captured stdout/stderr/exit-code from a single `git` invocation in a managed repo.

use std::{path::Path, process::Command};

use anyhow::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GitCapture {
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) code: i32,
}

impl GitCapture {
    pub(super) fn success(&self) -> bool {
        self.code == 0
    }

    pub(super) fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

pub(super) fn git_capture(repo: &Path, args: &[&str]) -> anyhow::Result<GitCapture> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo.display()))?;

    Ok(GitCapture {
        stdout: String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")?,
        stderr: String::from_utf8(output.stderr).context("git stderr was not valid UTF-8")?,
        code: output.status.code().unwrap_or(1),
    })
}
