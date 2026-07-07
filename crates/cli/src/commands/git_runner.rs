//! The git-execution seam shared by the local-git command flows (sw, sync,
//! prune, tag, worktree, squash-local): a mockable [`GitRunner`] trait, its
//! captured [`GitOutput`], the real [`StdGitRunner`] adapter, and the small
//! helpers those flows share.

use std::{path::Path, process::Command};

use anyhow::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    /// git's stderr (where it writes diagnostics on failure). Empty unless captured.
    pub stderr: String,
    pub exit_code: i32,
}

impl GitOutput {
    pub(crate) fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// git's own diagnostic — stderr (where it writes errors) if present, else stdout —
    /// trimmed. Empty when git said nothing.
    pub fn diagnostic(&self) -> &str {
        let stderr = self.stderr.trim();
        if stderr.is_empty() {
            self.stdout.trim()
        } else {
            stderr
        }
    }

    /// A failure detail: `context` plus git's own message when it gave one, otherwise
    /// `context (exit N)`. Lets every command surface git's real reason uniformly.
    pub fn fail_detail(&self, context: &str) -> String {
        match self.diagnostic() {
            said if !said.is_empty() => format!("{context}: {said}"),
            _ => format!("{context} (exit {})", self.exit_code),
        }
    }

    /// git's own message ([`GitOutput::diagnostic`]) if it gave one, else a generic line.
    /// Shared by every `apply_*` flow (switch, merge, branch, delete), so the fallback
    /// names no specific git subcommand.
    pub(crate) fn error_line(&self) -> String {
        match self.diagnostic() {
            said if !said.is_empty() => said.to_string(),
            _ => format!("git command failed (exit {})", self.exit_code),
        }
    }
}

pub trait GitRunner {
    fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput>;
}

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

/// Runs git and returns trimmed stdout on a clean exit, else `None`.
pub(crate) fn capture(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> Option<String> {
    match runner.run(repo, args) {
        Ok(out) if out.exit_code == 0 => Some(out.stdout.trim().to_string()),
        _ => None,
    }
}

/// True when `<onto>` exists as a local branch.
pub(crate) fn onto_exists(runner: &impl GitRunner, repo: &Path, onto: &str) -> bool {
    matches!(
        runner.run(repo, &["rev-parse", "--verify", &format!("refs/heads/{onto}")]),
        Ok(out) if out.exit_code == 0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_line_prefers_stderr_then_stdout_then_generic() {
        let with_stderr = GitOutput {
            stdout: "ignored".into(),
            stderr: "fatal: boom".into(),
            exit_code: 1,
        };
        assert_eq!(with_stderr.error_line(), "fatal: boom");

        let stdout_only = GitOutput {
            stdout: "some note".into(),
            stderr: String::new(),
            exit_code: 2,
        };
        assert_eq!(stdout_only.error_line(), "some note");

        let silent = GitOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 3,
        };
        assert_eq!(silent.error_line(), "git command failed (exit 3)");
    }
}
