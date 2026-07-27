use std::path::Path;

/// One captured git invocation: stdout, stderr, and the exit code, exactly as the
/// subprocess reported them. The [`GitRunner`] port's return value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    /// git's stderr (where it writes diagnostics on failure). Empty unless captured.
    pub stderr: String,
    pub exit_code: i32,
}

impl GitOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// git's own diagnostic - stderr (where it writes errors) if present, else stdout -
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
    pub fn error_line(&self) -> String {
        match self.diagnostic() {
            said if !said.is_empty() => said.to_string(),
            _ => format!("git command failed (exit {})", self.exit_code),
        }
    }

    /// stdout and stderr joined by a newline - the shape the managed `commit`
    /// fan-out parses for git's own last line (the retired `GitCapture::combined`).
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// The synchronous git-execution seam shared by the local-git command flows (sw,
/// sync, prune, tag, worktree, squash-local, push -r) and the in-process slices
/// behind them. One method mirroring one `git -C <repo> <args...>` invocation; all
/// interpretation of the captured output stays with the caller.
pub trait GitRunner: Clone + Send + Sync + 'static {
    fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput>;

    /// Whether `repo`'s `.git` entry exists on this machine - the sync twin of
    /// [`crate::ports::RemoteSync::repo_present`], and deliberately *not* a git invocation: an
    /// absent repo must classify as absent without spawning a subprocess. The
    /// default is the real fs check; fakes override it to script absence.
    fn repo_present(&self, repo: &Path) -> bool {
        repo.join(".git").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::GitOutput;

    #[test]
    fn git_output_error_line_prefers_stderr_then_stdout_then_generic() {
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
