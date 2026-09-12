//! Applies a confirmed current-repository push plan.

use gtl_models::{
    diffs::CommitId,
    git::{CommitCount, GitEffectMode, GitRange},
};

use super::plan_push::PushTarget;
use crate::ports::{GitClient, GitEffect, GitPushReceipt};

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

/// Reports the closed push status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyPushOk {
    /// Local changes prevent an existing-commits-only push.
    Refused { detail: String },
    /// Neither a local commit nor a remote update was needed.
    Noop { detail: String },
    /// The operation completed after attempting the remote push.
    Completed {
        detail: String,
        completion: PushCompletion,
    },
    /// Git rejected a mutating step after the reported partial progress.
    Failed {
        detail: String,
        progress: PushProgress,
    },
}

/// Identifies the local state sent by a remote push attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushBasis {
    /// The push sent commits that existed before this operation.
    ExistingCommits,
    /// The push sent a commit created by this operation.
    CreatedCommit { id: CommitId },
}

/// Reports how a completed remote push changed the destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushCompletion {
    /// The remote branch was updated.
    RemoteUpdated { basis: PushBasis },
    /// The remote branch already contained the requested state.
    RemoteAlreadyUpToDate { basis: PushBasis },
}

/// Reports the last completed local step or an indeterminate remote attempt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PushProgress {
    /// The operation has not changed the repository.
    #[default]
    NotStarted,
    /// All working-tree changes were staged.
    Staged,
    /// A local commit was created but no remote push was attempted.
    CommitCreated { id: CommitId },
    /// A remote push was attempted; transport failure may leave its effect unknown.
    PushAttempted { basis: PushBasis },
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
    Clean { ahead: CommitCount },
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
        GitEffect::Rejected(detail) => {
            return Ok(ApplyPushOk::Failed {
                detail: super::git_failure("git status", &detail),
                progress,
            });
        }
    };

    let state = if working_tree.files.is_empty() {
        match git
            .commit_count(&target.top, &GitRange::upstream_to_head())
            .map_err(|source| transport("count unpushed commits", progress.clone(), source))?
        {
            Some(ahead) => RepositoryState::Clean { ahead },
            None => {
                return Ok(ApplyPushOk::Failed {
                    detail: "rev-list failed".into(),
                    progress,
                });
            }
        }
    } else {
        RepositoryState::Dirty
    };

    match classify(state, mode) {
        PushAction::Refuse => Ok(ApplyPushOk::Refused {
            detail: "working tree has uncommitted changes".into(),
        }),
        PushAction::Noop { detail } => Ok(ApplyPushOk::Noop {
            detail: detail.into(),
        }),
        PushAction::Push { detail } => {
            let basis = PushBasis::ExistingCommits;
            let effect = push(git, &target, &basis, &mut progress)?;
            Ok(finish_push(effect, detail, "already up to date", basis))
        }
        PushAction::CommitAndPush { message } => {
            if let GitEffect::Rejected(detail) = git
                .stage_all(&target.top)
                .map_err(|source| transport("stage changes", progress.clone(), source))?
            {
                return Ok(ApplyPushOk::Failed {
                    detail: super::git_failure("git add", &detail),
                    progress,
                });
            }
            progress = PushProgress::Staged;
            let receipt = match git
                .commit(&target.top, &message)
                .map_err(|source| transport("create commit", progress.clone(), source))?
            {
                GitEffect::Applied(receipt) => receipt,
                GitEffect::Rejected(detail) => {
                    return Ok(ApplyPushOk::Failed {
                        detail: super::git_failure("git commit", &detail),
                        progress,
                    });
                }
            };
            let id = receipt.id;
            progress = PushProgress::CommitCreated { id: id.clone() };
            let basis = PushBasis::CreatedCommit { id };
            let effect = push(git, &target, &basis, &mut progress)?;
            Ok(finish_push(
                effect,
                "staged, committed, and pushed".into(),
                "staged and committed; remote already up to date",
                basis,
            ))
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

    if ahead == CommitCount::default() {
        return PushAction::Noop {
            detail: match mode {
                PushMode::ExistingOnly => "already up to date",
                PushMode::CommitChanges { .. } => "nothing to commit; already up to date",
            },
        };
    }

    let noun = if ahead.into_inner() == 1 {
        "commit"
    } else {
        "commits"
    };
    PushAction::Push {
        detail: format!("pushed {ahead} {noun}"),
    }
}

fn push(
    git: &impl GitClient,
    target: &PushTarget,
    basis: &PushBasis,
    progress: &mut PushProgress,
) -> Result<GitEffect<GitPushReceipt>, ApplyPushError> {
    *progress = PushProgress::PushAttempted {
        basis: basis.clone(),
    };
    git.push_branch(
        &target.top,
        &target.remote,
        &target.branch,
        GitEffectMode::Apply,
    )
    .map_err(|source| transport("push branch", progress.clone(), source))
}

fn finish_push(
    effect: GitEffect<GitPushReceipt>,
    updated_detail: String,
    up_to_date_detail: &str,
    basis: PushBasis,
) -> ApplyPushOk {
    match effect {
        GitEffect::Applied(GitPushReceipt::Updated { .. }) => ApplyPushOk::Completed {
            detail: updated_detail,
            completion: PushCompletion::RemoteUpdated { basis },
        },
        GitEffect::Applied(GitPushReceipt::UpToDate { .. }) => ApplyPushOk::Completed {
            detail: up_to_date_detail.into(),
            completion: PushCompletion::RemoteAlreadyUpToDate { basis },
        },
        GitEffect::Rejected(detail) => ApplyPushOk::Failed {
            detail: if detail.is_empty() {
                "push failed".into()
            } else {
                detail
            },
            progress: PushProgress::PushAttempted { basis },
        },
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
    use std::error::Error as _;

    use gtl_models::{git::RemoteUrl, repository::PendingChanges};

    use super::*;
    use crate::{
        repositories::apply_push,
        utils::{ScriptedGitClient, branch_name, remote_name},
    };

    fn target() -> PushTarget {
        PushTarget {
            name: crate::utils::project_name("api"),
            top: crate::utils::repository_root("/repos/api"),
            branch: branch_name("main"),
            remote: remote_name("origin"),
            remote_urls: vec![
                RemoteUrl::try_new("git@example.invalid:team/example-project.git").unwrap(),
            ],
            pending: PendingChanges::default(),
        }
    }

    #[test]
    fn clean_existing_only_push_classifies_ahead_as_push() {
        let action = classify(
            RepositoryState::Clean {
                ahead: CommitCount::new(2),
            },
            PushMode::ExistingOnly,
        );

        assert_eq!(
            action,
            PushAction::Push {
                detail: "pushed 2 commits".into()
            }
        );
    }

    #[test]
    fn clean_commit_changes_push_classifies_zero_ahead_as_noop() {
        let action = classify(
            RepositoryState::Clean {
                ahead: CommitCount::default(),
            },
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

        let result = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
            },
            &git,
        )
        .unwrap();

        assert_eq!(
            result,
            ApplyPushOk::Failed {
                detail: "rev-list failed".into(),
                progress: PushProgress::default(),
            }
        );
    }

    #[test]
    fn dirty_existing_only_push_returns_a_closed_refusal() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied(" M src/lib.rs\n")]);

        let result = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
            },
            &git,
        )
        .unwrap();

        assert_eq!(
            result,
            ApplyPushOk::Refused {
                detail: "working tree has uncommitted changes".into(),
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

        let result = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::CommitChanges {
                    message: "save work".into(),
                },
            },
            &git,
        )
        .unwrap();

        assert_eq!(
            result,
            ApplyPushOk::Failed {
                detail: "git commit failed: commit rejected".into(),
                progress: PushProgress::Staged,
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

        let result = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::CommitChanges {
                    message: "save work".into(),
                },
            },
            &git,
        )
        .unwrap();

        let progress = match result {
            ApplyPushOk::Failed { progress, .. } => Some(progress),
            ApplyPushOk::Refused { .. }
            | ApplyPushOk::Noop { .. }
            | ApplyPushOk::Completed { .. } => None,
        }
        .unwrap();
        assert_eq!(
            progress,
            PushProgress::PushAttempted {
                basis: PushBasis::CreatedCommit {
                    id: crate::utils::commit_id_fixture("abc1234"),
                },
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

        let error = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::CommitChanges {
                    message: "save work".into(),
                },
            },
            &git,
        )
        .unwrap_err();

        let ApplyPushError::Transport {
            progress, source, ..
        } = error;
        assert_eq!(
            progress,
            PushProgress::PushAttempted {
                basis: PushBasis::CreatedCommit {
                    id: crate::utils::commit_id_fixture("abc1234"),
                },
            }
        );
        assert_eq!(source.to_string(), "push transport unavailable");
    }

    #[test]
    fn transport_failure_remains_a_sourced_apply_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
            },
            &git,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "read working tree: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    #[test]
    fn successful_existing_commit_push_reports_the_remote_update() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("2\n"),
            ScriptedGitClient::applied("pushed\n"),
        ]);

        let result = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
            },
            &git,
        )
        .unwrap();

        assert_eq!(
            result,
            ApplyPushOk::Completed {
                detail: "pushed 2 commits".into(),
                completion: PushCompletion::RemoteUpdated {
                    basis: PushBasis::ExistingCommits,
                },
            }
        );
    }

    #[test]
    fn remote_race_reports_an_attempt_that_was_already_up_to_date() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("2\n"),
            ScriptedGitClient::applied("Everything up-to-date\n"),
        ]);

        let result = apply_push::execute(
            ApplyPush {
                target: target(),
                mode: PushMode::ExistingOnly,
            },
            &git,
        )
        .unwrap();

        assert_eq!(
            result,
            ApplyPushOk::Completed {
                detail: "already up to date".into(),
                completion: PushCompletion::RemoteAlreadyUpToDate {
                    basis: PushBasis::ExistingCommits,
                },
            }
        );
    }
}
