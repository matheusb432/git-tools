//! Applies a confirmed local-only commit plan.

use super::{commit_progress::CommitProgress, plan_commit::CommitTarget};
use crate::{
    ports::GitRunner,
    shared::git::{command_label, created_commit_identity},
};

/// Requests staging and committing the confirmed target without pushing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCommit {
    pub target: CommitTarget,
    pub message: String,
}

/// Classifies the completed local commit for CLI exit and output mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitStatus {
    Noop,
    Committed,
    Failed,
}

/// Reports the closed local commit status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    pub status: CommitStatus,
    pub detail: String,
    pub progress: CommitProgress,
}

impl CommitResult {
    fn new(status: CommitStatus, detail: impl Into<String>, progress: CommitProgress) -> Self {
        Self {
            status,
            detail: detail.into(),
            progress,
        }
    }
}

/// Reports an unexpected Git transport failure while applying a local commit.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyCommitError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        progress: CommitProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Revalidates the target, stages all changes, and creates one local commit.
///
/// # Errors
///
/// Returns [`ApplyCommitError`] when Git transport fails.
#[cqrsy::handler(command)]
pub fn execute(
    command: ApplyCommit,
    git: &impl GitRunner,
) -> Result<CommitResult, ApplyCommitError> {
    let ApplyCommit { target, message } = command;
    let mut progress = CommitProgress::Unchanged;
    let status_args = ["status", "--porcelain"];
    let dirty = match git
        .run(&target.top, &status_args)
        .map_err(|source| transport(&status_args, progress.clone(), source))?
    {
        output if output.exit_code == 0 => !output.stdout.trim().is_empty(),
        _ => {
            return Ok(CommitResult::new(
                CommitStatus::Failed,
                "git status failed",
                progress,
            ));
        }
    };

    if !dirty {
        return Ok(CommitResult::new(
            CommitStatus::Noop,
            "nothing to commit",
            progress,
        ));
    }
    let add_args = ["add", "-A"];
    let add = git
        .run(&target.top, &add_args)
        .map_err(|source| transport(&add_args, progress.clone(), source))?;
    if !add.success() {
        return Ok(CommitResult::new(
            CommitStatus::Failed,
            "git add failed",
            progress,
        ));
    }
    progress = CommitProgress::Staged;
    let commit_args = ["commit", "-m", message.as_str()];
    let commit = git
        .run(&target.top, &commit_args)
        .map_err(|source| transport(&commit_args, progress.clone(), source))?;
    if !commit.success() {
        return Ok(CommitResult::new(
            CommitStatus::Failed,
            "git commit failed",
            progress,
        ));
    }
    progress = CommitProgress::Created {
        identity: created_commit_identity(&commit),
    };

    Ok(CommitResult::new(
        CommitStatus::Committed,
        "staged and committed",
        progress,
    ))
}

fn transport(args: &[&str], progress: CommitProgress, source: anyhow::Error) -> ApplyCommitError {
    ApplyCommitError::Transport {
        command: command_label(args),
        progress,
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{repository_sync::PendingChanges, testing::FakeGitRunner};

    fn target() -> CommitTarget {
        CommitTarget {
            name: "api".into(),
            top: "/repos/api".into(),
            branch: "main".into(),
            pending: PendingChanges::default(),
        }
    }

    #[test]
    fn clean_commit_returns_a_closed_noop() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok("")]);

        let result = execute(
            ApplyCommit {
                target: target(),
                message: "save work".into(),
            },
            &git,
        )
        .expect("a clean tree is an expected no-op");

        assert_eq!(
            result,
            CommitResult {
                status: CommitStatus::Noop,
                detail: "nothing to commit".into(),
                progress: CommitProgress::Unchanged,
            }
        );
    }

    #[test]
    fn failed_add_returns_a_closed_failure() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(" M src/lib.rs\n"),
            FakeGitRunner::exit_err("add rejected", 1),
        ]);

        let result = execute(
            ApplyCommit {
                target: target(),
                message: "save work".into(),
            },
            &git,
        )
        .expect("a rejected add is a closed failure");

        assert_eq!(
            result,
            CommitResult {
                status: CommitStatus::Failed,
                detail: "git add failed".into(),
                progress: CommitProgress::Unchanged,
            }
        );
    }

    #[test]
    fn rejected_commit_reports_that_staging_completed() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(" M src/lib.rs\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("commit rejected", 1),
        ]);

        let result = execute(
            ApplyCommit {
                target: target(),
                message: "save work".into(),
            },
            &git,
        )
        .expect("a rejected commit is a closed failure");

        assert_eq!(result.status, CommitStatus::Failed);
        assert_eq!(result.detail, "git commit failed");
        assert_eq!(result.progress, CommitProgress::Staged);
    }

    #[test]
    fn successful_commit_reports_creation_with_parsed_identity() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(" M src/lib.rs\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok("[main abc1234] save work\n"),
        ]);

        let result = execute(
            ApplyCommit {
                target: target(),
                message: "save work".into(),
            },
            &git,
        )
        .expect("commit succeeds");

        assert_eq!(
            result.progress,
            CommitProgress::Created {
                identity: Some("abc1234".into()),
            }
        );
    }

    #[test]
    fn commit_transport_after_staging_preserves_progress_and_source() {
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok(" M src/lib.rs\n")),
            Ok(FakeGitRunner::ok("")),
            Err(anyhow::anyhow!("commit transport unavailable")),
        ]);

        let error = execute(
            ApplyCommit {
                target: target(),
                message: "save work".into(),
            },
            &git,
        )
        .expect_err("commit transport fails");

        let ApplyCommitError::Transport {
            progress, source, ..
        } = error;
        assert_eq!(progress, CommitProgress::Staged);
        assert_eq!(source.to_string(), "commit transport unavailable");
    }

    #[test]
    fn transport_failure_remains_a_sourced_apply_error() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(
            ApplyCommit {
                target: target(),
                message: "save work".into(),
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(
            error.to_string(),
            "git status --porcelain: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
