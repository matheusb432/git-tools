//! Applies a planned recovery of a branch's prior position.

use super::{BranchRecovery, plan_revert::RevertTarget};
use crate::ports::{GitClient, GitEffect};

/// Requests applying one confirmed branch recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyRevert {
    pub target: RevertTarget,
}

/// Classifies the result of applying a branch recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertStatus {
    Reverted,
    Failed,
}

/// Reports the closed branch recovery status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyRevertOk {
    pub status: RevertStatus,
    pub detail: String,
    pub progress: RevertProgress,
}

/// Reports completed branch-recovery steps and available recovery data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RevertProgress {
    pub branch_switched: bool,
    pub force_move_done: bool,
    pub recovery: Option<BranchRecovery>,
}

impl ApplyRevertOk {
    fn new(status: RevertStatus, detail: impl Into<String>, progress: RevertProgress) -> Self {
        Self {
            status,
            detail: detail.into(),
            progress,
        }
    }
}

/// Reports an unexpected Git transport failure while applying branch recovery.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyRevertError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        progress: RevertProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Switches back and restores the target branch's prior position.
///
/// # Errors
///
/// Returns [`ApplyRevertError`] when Git transport fails.
#[cqrsy::command]
pub fn execute(
    command: ApplyRevert,
    git: &impl GitClient,
) -> Result<ApplyRevertOk, ApplyRevertError> {
    let ApplyRevert { target } = command;
    let mut progress = RevertProgress::default();
    match git.switch_previous(&target.top) {
        Ok(GitEffect::Applied(())) => {}
        Ok(GitEffect::Rejected(detail)) => {
            return Ok(ApplyRevertOk::new(RevertStatus::Failed, detail, progress));
        }
        Err(source) => return Err(transport("switch to previous branch", progress, source)),
    }
    progress.branch_switched = true;
    progress.recovery = Some(BranchRecovery::switch_to(&target.onto));

    match git.move_branch(&target.top, &target.onto, target.prior_id.as_ref()) {
        Ok(GitEffect::Applied(())) => {
            progress.force_move_done = true;
            progress.recovery = None;
            Ok(ApplyRevertOk::new(
                RevertStatus::Reverted,
                format!(
                    "reverted '{}' to {} and switched back",
                    target.onto, target.prior_id
                ),
                progress,
            ))
        }
        Ok(GitEffect::Rejected(detail)) => {
            Ok(ApplyRevertOk::new(RevertStatus::Failed, detail, progress))
        }
        Err(source) => Err(transport("move branch", progress, source)),
    }
}

fn transport(command: &str, progress: RevertProgress, source: anyhow::Error) -> ApplyRevertError {
    ApplyRevertError::Transport {
        command: command.to_string(),
        progress,
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{branches::plan_revert::RevertTarget, testing::ScriptedGitClient};

    #[test]
    fn successful_revert_reports_the_recovery_position() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("switched\n"),
            ScriptedGitClient::applied(""),
        ]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_id: crate::testing::commit_id_fixture("abc123"),
        };

        let result =
            execute(ApplyRevert { target }, &git).expect("branch recovery application succeeds");

        assert_eq!(
            result,
            ApplyRevertOk {
                status: RevertStatus::Reverted,
                detail:
                    "reverted 'main' to abc123abc123abc123abc123abc123abc123abc1 and switched back"
                        .into(),
                progress: RevertProgress {
                    branch_switched: true,
                    force_move_done: true,
                    recovery: None,
                },
            }
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_apply_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_id: crate::testing::commit_id_fixture("abc123"),
        };

        let error = execute(ApplyRevert { target }, &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(
            error.to_string(),
            "switch to previous branch: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    #[test]
    fn failed_force_move_reports_switch_and_recovery_command() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("switched\n"),
            ScriptedGitClient::rejected("branch locked"),
        ]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_id: crate::testing::commit_id_fixture("abc123"),
        };

        let result = execute(ApplyRevert { target }, &git)
            .expect("a rejected force move is a closed failure");

        assert_eq!(result.status, RevertStatus::Failed);
        assert_eq!(
            result.progress,
            RevertProgress {
                branch_switched: true,
                force_move_done: false,
                recovery: Some(BranchRecovery {
                    original_branch: "main".into(),
                    command: "git switch main".into(),
                }),
            }
        );
    }

    #[test]
    fn force_move_transport_reports_switch_recovery_and_source() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("switched\n")),
            Err(anyhow::anyhow!("branch transport unavailable")),
        ]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_id: crate::testing::commit_id_fixture("abc123"),
        };

        let error = execute(ApplyRevert { target }, &git).expect_err("force-move transport fails");
        let ApplyRevertError::Transport {
            progress, source, ..
        } = error;

        assert_eq!(
            progress,
            RevertProgress {
                branch_switched: true,
                force_move_done: false,
                recovery: Some(BranchRecovery {
                    original_branch: "main".into(),
                    command: "git switch main".into(),
                }),
            }
        );
        assert_eq!(source.to_string(), "branch transport unavailable");
    }
}
