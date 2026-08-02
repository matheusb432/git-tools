//! Applies a confirmed local-only commit plan.

use super::{commit_progress::CommitProgress, plan_commit::CommitTarget};
use crate::ports::{GitClient, GitEffect};

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
pub struct ApplyCommitOk {
    pub status: CommitStatus,
    pub detail: String,
    pub progress: CommitProgress,
}

impl ApplyCommitOk {
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
#[cqrsy::command]
pub fn execute(
    command: ApplyCommit,
    git: &impl GitClient,
) -> Result<ApplyCommitOk, ApplyCommitError> {
    let ApplyCommit { target, message } = command;
    let mut progress = CommitProgress::Unchanged;
    let dirty = match git
        .working_tree(&target.top)
        .map_err(|source| transport("read working tree", progress.clone(), source))?
    {
        GitEffect::Applied(tree) => !tree.files.is_empty(),
        GitEffect::Rejected(_) => {
            return Ok(ApplyCommitOk::new(
                CommitStatus::Failed,
                "git status failed",
                progress,
            ));
        }
    };

    if !dirty {
        return Ok(ApplyCommitOk::new(
            CommitStatus::Noop,
            "nothing to commit",
            progress,
        ));
    }
    if let GitEffect::Rejected(_) = git
        .stage_all(&target.top)
        .map_err(|source| transport("stage changes", progress.clone(), source))?
    {
        return Ok(ApplyCommitOk::new(
            CommitStatus::Failed,
            "git add failed",
            progress,
        ));
    }
    progress = CommitProgress::Staged;
    let receipt = match git
        .commit(&target.top, &message)
        .map_err(|source| transport("create commit", progress.clone(), source))?
    {
        GitEffect::Applied(receipt) => receipt,
        GitEffect::Rejected(_) => {
            return Ok(ApplyCommitOk::new(
                CommitStatus::Failed,
                "git commit failed",
                progress,
            ));
        }
    };
    progress = CommitProgress::Created {
        identity: receipt.identity,
    };

    Ok(ApplyCommitOk::new(
        CommitStatus::Committed,
        "staged and committed",
        progress,
    ))
}

fn transport(command: &str, progress: CommitProgress, source: anyhow::Error) -> ApplyCommitError {
    ApplyCommitError::Transport {
        command: command.into(),
        progress,
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use gtl_models::repository::PendingChanges;

    use super::*;
    use crate::testing::ScriptedGitClient;

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
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

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
            ApplyCommitOk {
                status: CommitStatus::Noop,
                detail: "nothing to commit".into(),
                progress: CommitProgress::Unchanged,
            }
        );
    }

    #[test]
    fn failed_add_returns_a_closed_failure() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::rejected("add rejected"),
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
            ApplyCommitOk {
                status: CommitStatus::Failed,
                detail: "git add failed".into(),
                progress: CommitProgress::Unchanged,
            }
        );
    }

    #[test]
    fn rejected_commit_reports_that_staging_completed() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("commit rejected"),
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
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("[main abc1234] save work\n"),
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
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied(" M src/lib.rs\n")),
            Ok(ScriptedGitClient::applied("")),
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
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

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
            "read working tree: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
