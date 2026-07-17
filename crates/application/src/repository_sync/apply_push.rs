//! Applies a confirmed current-repository push plan.

use super::{commit_progress::CommitProgress, plan_push::PushTarget};
use crate::{
    ports::GitRunner,
    shared::git::{capture_checked, command_label, created_commit_identity},
};

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
pub struct PushResult {
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

impl PushResult {
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
#[cqrsy::handler(command)]
pub fn execute(command: ApplyPush, git: &impl GitRunner) -> Result<PushResult, ApplyPushError> {
    let ApplyPush { target, mode } = command;
    let mut progress = PushProgress::default();
    let status_args = ["status", "--porcelain"];
    let porcelain = match git
        .run(&target.top, &status_args)
        .map_err(|source| transport(&status_args, progress.clone(), source))?
    {
        output if output.exit_code == 0 => output.stdout,
        _ => {
            return Ok(PushResult::new(
                PushStatus::Failed,
                "git status failed",
                progress,
            ));
        }
    };

    let state = if porcelain.trim().is_empty() {
        let ahead_args = ["rev-list", "--count", "@{u}..HEAD"];
        match capture_checked(git, &target.top, &ahead_args)
            .map_err(|source| transport(&ahead_args, progress.clone(), source))?
            .filter(|count| !count.is_empty())
        {
            Some(count) => RepositoryState::Clean {
                ahead: count.parse::<usize>().unwrap_or(0),
            },
            None => {
                return Ok(PushResult::new(
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
        PushAction::Refuse => Ok(PushResult::new(
            PushStatus::Refused,
            "working tree has uncommitted changes",
            progress,
        )),
        PushAction::Noop { detail } => Ok(PushResult::new(PushStatus::Noop, detail, progress)),
        PushAction::Push { detail } => match push(git, &target, &progress)? {
            None => {
                progress.pushed = true;
                Ok(PushResult::new(PushStatus::Pushed, detail, progress))
            }
            Some(detail) => Ok(PushResult::new(PushStatus::Failed, detail, progress)),
        },
        PushAction::CommitAndPush { message } => {
            let add_args = ["add", "-A"];
            let add = git
                .run(&target.top, &add_args)
                .map_err(|source| transport(&add_args, progress.clone(), source))?;
            if !add.success() {
                return Ok(PushResult::new(
                    PushStatus::Failed,
                    "git add failed",
                    progress,
                ));
            }
            progress.commit = CommitProgress::Staged;
            let commit_args = ["commit", "-m", message.as_str()];
            let commit = git
                .run(&target.top, &commit_args)
                .map_err(|source| transport(&commit_args, progress.clone(), source))?;
            if !commit.success() {
                return Ok(PushResult::new(
                    PushStatus::Failed,
                    "git commit failed",
                    progress,
                ));
            }
            progress.commit = CommitProgress::Created {
                identity: created_commit_identity(&commit),
            };
            match push(git, &target, &progress)? {
                None => {
                    progress.pushed = true;
                    Ok(PushResult::new(
                        PushStatus::Pushed,
                        "staged, committed, and pushed",
                        progress,
                    ))
                }
                Some(detail) => Ok(PushResult::new(PushStatus::Failed, detail, progress)),
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
    git: &impl GitRunner,
    target: &PushTarget,
    progress: &PushProgress,
) -> Result<Option<String>, ApplyPushError> {
    let args = ["push", target.remote.as_str(), target.branch.as_str()];
    match git
        .run(&target.top, &args)
        .map_err(|source| transport(&args, progress.clone(), source))?
    {
        output if output.exit_code == 0 => Ok(None),
        output => Ok(Some(output.fail_detail("push failed"))),
    }
}

fn transport(args: &[&str], progress: PushProgress, source: anyhow::Error) -> ApplyPushError {
    ApplyPushError::Transport {
        command: command_label(args),
        progress,
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, path::PathBuf};

    use domain::repository::PendingChanges;

    use super::*;
    use crate::testing::FakeGitRunner;

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
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok(""), FakeGitRunner::ok("")]);

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
            PushResult {
                status: PushStatus::Failed,
                detail: "rev-list failed".into(),
                progress: PushProgress::default(),
            }
        );
    }

    #[test]
    fn dirty_existing_only_push_returns_a_closed_refusal() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok(" M src/lib.rs\n")]);

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
            PushResult {
                status: PushStatus::Refused,
                detail: "working tree has uncommitted changes".into(),
                progress: PushProgress::default(),
            }
        );
    }

    #[test]
    fn failed_commit_returns_a_closed_failure() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(" M src/lib.rs\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("commit rejected", 1),
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
            PushResult {
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
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok(" M src/lib.rs\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok("[main abc1234] save work\n"),
            FakeGitRunner::exit_err("remote rejected", 1),
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
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok(" M src/lib.rs\n")),
            Ok(FakeGitRunner::ok("")),
            Ok(FakeGitRunner::ok("[main abc1234] save work\n")),
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
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

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
            "git status --porcelain: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
