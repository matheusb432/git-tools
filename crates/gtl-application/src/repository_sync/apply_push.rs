//! Applies a confirmed current-repository push plan.

use super::{logic::commit_progress::CommitProgress, plan_push::PushTarget};
use crate::ports::{GitClient, GitEffect};

/// Selects whether a push may create a commit or may push existing commits only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushMode {
    ExistingOnly,
    CommitChanges { message: String },
}

/// Requests application of a previously confirmed push target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPush {
    pub target: PushTarget,
    pub mode: PushMode,
}

/// Classifies the completed push attempt for CLI exit and output mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushStatus {
    Refused,
    Noop,
    Pushed,
    Failed,
}

/// Reports the closed push status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPushOk {
    pub status: PushStatus,
    pub detail: String,
    pub progress: PushProgress,
}

/// Reports the completed local-commit and remote-push steps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushProgress {
    pub commit: CommitProgress,
    pub pushed: bool,
}

impl ApplyPushOk {
    fn new(status: PushStatus, detail: impl Into<String>, progress: PushProgress) -> Self {
        Self {
            status,
            detail: detail.into(),
            progress,
        }
    }
}

/// Reports an unexpected Git transport failure while applying a push.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyPushError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        progress: PushProgress,
        #[source]
        source: anyhow::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PushAction {
    Refuse,
    Noop { detail: &'static str },
    Push { detail: String },
    CommitAndPush { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepositoryState {
    Dirty,
    Clean { ahead: usize },
}

/// Revalidates the target, optionally stages and commits changes, and pushes its branch.
///
/// # Errors
///
/// Returns [`ApplyPushError`] when Git transport fails.
#[cqrsy::command]
pub fn execute(command: ApplyPush, git: &impl GitClient) -> Result<ApplyPushOk, ApplyPushError> {
    let ApplyPush { target, mode } = command;
    let mut progress = PushProgress::default();
    let working_tree = match git
        .working_tree(&target.top)
        .map_err(|source| transport("read working tree", progress.clone(), source))?
    {
        GitEffect::Applied(working_tree) => working_tree,
        GitEffect::Rejected(_) => {
            return Ok(ApplyPushOk::new(
                PushStatus::Failed,
                "git status failed",
                progress,
            ));
        }
    };

    let state = if working_tree.files.is_empty() {
        match git
            .commit_count(&target.top, "@{u}..HEAD")
            .map_err(|source| transport("count unpushed commits", progress.clone(), source))?
        {
            Some(ahead) => RepositoryState::Clean { ahead },
            None => {
                return Ok(ApplyPushOk::new(
                    PushStatus::Failed,
                    "rev-list failed",
                    progress,
                ));
            }
        }
    } else {
        RepositoryState::Dirty
    };

    match classify(state, mode) {
        PushAction::Refuse => Ok(ApplyPushOk::new(
            PushStatus::Refused,
            "working tree has uncommitted changes",
            progress,
        )),
        PushAction::Noop { detail } => Ok(ApplyPushOk::new(PushStatus::Noop, detail, progress)),
        PushAction::Push { detail } => match push(git, &target, &progress)? {
            None => {
                progress.pushed = true;
                Ok(ApplyPushOk::new(PushStatus::Pushed, detail, progress))
            }
            Some(detail) => Ok(ApplyPushOk::new(PushStatus::Failed, detail, progress)),
        },
        PushAction::CommitAndPush { message } => {
            if let GitEffect::Rejected(_) = git
                .stage_all(&target.top)
                .map_err(|source| transport("stage changes", progress.clone(), source))?
            {
                return Ok(ApplyPushOk::new(
                    PushStatus::Failed,
                    "git add failed",
                    progress,
                ));
            }
            progress.commit = CommitProgress::Staged;
            let receipt = match git
                .commit(&target.top, &message)
                .map_err(|source| transport("create commit", progress.clone(), source))?
            {
                GitEffect::Applied(receipt) => receipt,
                GitEffect::Rejected(_) => {
                    return Ok(ApplyPushOk::new(
                        PushStatus::Failed,
                        "git commit failed",
                        progress,
                    ));
                }
            };
            progress.commit = CommitProgress::Created {
                identity: receipt.identity,
            };
            match push(git, &target, &progress)? {
                None => {
                    progress.pushed = true;
                    Ok(ApplyPushOk::new(
                        PushStatus::Pushed,
                        "staged, committed, and pushed",
                        progress,
                    ))
                }
                Some(detail) => Ok(ApplyPushOk::new(PushStatus::Failed, detail, progress)),
            }
        }
    }
}

fn classify(state: RepositoryState, mode: PushMode) -> PushAction {
    let ahead = match state {
        RepositoryState::Dirty => {
            return match mode {
                PushMode::ExistingOnly => PushAction::Refuse,
                PushMode::CommitChanges { message } => PushAction::CommitAndPush { message },
            };
        }
        RepositoryState::Clean { ahead } => ahead,
    };

    if ahead == 0 {
        return PushAction::Noop {
            detail: match mode {
                PushMode::ExistingOnly => "already up to date",
                PushMode::CommitChanges { .. } => "nothing to commit; already up to date",
            },
        };
    }

    PushAction::Push {
        detail: match mode {
            PushMode::ExistingOnly => format!("pushed {ahead} commit(s)"),
            PushMode::CommitChanges { .. } => {
                format!("nothing to commit; pushed {ahead} commit(s)")
            }
        },
    }
}

fn push(
    git: &impl GitClient,
    target: &PushTarget,
    progress: &PushProgress,
) -> Result<Option<String>, ApplyPushError> {
    match git
        .push_branch(&target.top, &target.remote, &target.branch, false)
        .map_err(|source| transport("push branch", progress.clone(), source))?
    {
        GitEffect::Applied(_) => Ok(None),
        GitEffect::Rejected(detail) => Ok(Some(if detail.is_empty() {
            "push failed".into()
        } else {
            detail
        })),
    }
}

fn transport(command: &str, progress: PushProgress, source: anyhow::Error) -> ApplyPushError {
    ApplyPushError::Transport {
        command: command.into(),
        progress,
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, path::PathBuf};

    use gtl_models::repository::PendingChanges;

    use super::*;
    use crate::testing::ScriptedGitClient;

    fn target() -> PushTarget {
        PushTarget {
            name: "api".into(),
            top: PathBuf::from("/repos/api"),
            branch: "main".into(),
            remote: "origin".into(),
            remote_url: "git@example.com:team/api.git".into(),
            pending: PendingChanges::default(),
        }
    }

    #[test]
    fn clean_existing_only_push_classifies_ahead_as_push() {
        let action = classify(RepositoryState::Clean { ahead: 2 }, PushMode::ExistingOnly);

        assert_eq!(
            action,
            PushAction::Push {
                detail: "pushed 2 commit(s)".into()
            }
        );
    }

    #[test]
    fn clean_commit_changes_push_classifies_zero_ahead_as_noop() {
        let action = classify(
            RepositoryState::Clean { ahead: 0 },
            PushMode::CommitChanges {
                message: "save work".into(),
            },
        );

        assert_eq!(
            action,
            PushAction::Noop {
                detail: "nothing to commit; already up to date"
            }
        );
    }

    #[test]
    fn unavailable_ahead_count_returns_a_closed_failure() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
        ]);

        let result = execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
            },
            &git,
        )
        .expect("an unavailable ahead count is a closed failure");

        assert_eq!(
            result,
            ApplyPushOk {
                status: PushStatus::Failed,
                detail: "rev-list failed".into(),
                progress: PushProgress::default(),
            }
        );
    }

    #[test]
    fn dirty_existing_only_push_returns_a_closed_refusal() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied(" M src/lib.rs\n")]);

        let result = execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
            },
            &git,
        )
        .expect("a dirty existing-only push is a closed refusal");

        assert_eq!(
            result,
            ApplyPushOk {
                status: PushStatus::Refused,
                detail: "working tree has uncommitted changes".into(),
                progress: PushProgress::default(),
            }
        );
    }

    #[test]
    fn failed_commit_returns_a_closed_failure() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("commit rejected"),
        ]);

        let result = execute(
            ApplyPush {
                target: target(),
                mode: PushMode::CommitChanges {
                    message: "save work".into(),
                },
            },
            &git,
        )
        .expect("a rejected commit is a closed failure");

        assert_eq!(
            result,
            ApplyPushOk {
                status: PushStatus::Failed,
                detail: "git commit failed".into(),
                progress: PushProgress {
                    commit: CommitProgress::Staged,
                    pushed: false,
                },
            }
        );
    }

    #[test]
    fn rejected_push_preserves_the_created_commit_identity() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(" M src/lib.rs\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("[main abc1234] save work\n"),
            ScriptedGitClient::rejected("remote rejected"),
        ]);

        let result = execute(
            ApplyPush {
                target: target(),
                mode: PushMode::CommitChanges {
                    message: "save work".into(),
                },
            },
            &git,
        )
        .expect("a rejected push is a closed failure");

        assert_eq!(result.status, PushStatus::Failed);
        assert_eq!(
            result.progress,
            PushProgress {
                commit: CommitProgress::Created {
                    identity: Some("abc1234".into()),
                },
                pushed: false,
            }
        );
    }

    #[test]
    fn push_transport_after_commit_preserves_completed_progress_and_source() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied(" M src/lib.rs\n")),
            Ok(ScriptedGitClient::applied("")),
            Ok(ScriptedGitClient::applied("[main abc1234] save work\n")),
            Err(anyhow::anyhow!("push transport unavailable")),
        ]);

        let error = execute(
            ApplyPush {
                target: target(),
                mode: PushMode::CommitChanges {
                    message: "save work".into(),
                },
            },
            &git,
        )
        .expect_err("push transport fails");

        let ApplyPushError::Transport {
            progress, source, ..
        } = error;
        assert_eq!(
            progress,
            PushProgress {
                commit: CommitProgress::Created {
                    identity: Some("abc1234".into()),
                },
                pushed: false,
            }
        );
        assert_eq!(source.to_string(), "push transport unavailable");
    }

    #[test]
    fn transport_failure_remains_a_sourced_apply_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
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
