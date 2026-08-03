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
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo_path.display()))?;

    Ok(GitProcessOutput {
        stdout: String::from_utf8(output.stdout).context("git stdout was not valid UTF-8")?,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(1),
    })
}

fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
}
