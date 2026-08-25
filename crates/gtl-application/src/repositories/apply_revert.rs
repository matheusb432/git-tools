//! Applies a planned recovery of a branch's prior position.

use super::{BranchRecovery, plan_revert::RevertTarget};
use crate::ports::{GitClient, GitEffect};

/// Reports the closed branch recovery status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyRevertOk {
    /// The prior branch position was restored.
    Reverted { detail: String },
    /// Git rejected a recovery step after the reported partial progress.
    Failed {
        detail: String,
        progress: RevertProgress,
    },
}

/// Reports the last completed branch-recovery step and its recovery data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RevertProgress {
    /// The operation has not changed the repository.
    #[default]
    NotStarted,
    /// The previous branch is checked out and the target branch can be restored.
    BranchSwitched { recovery: BranchRecovery },
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
#[expect(
    clippy::needless_pass_by_value,
    reason = "CQRsy operations own their request value"
)]
pub fn execute(
    target: RevertTarget,
    git: &impl GitClient,
) -> Result<ApplyRevertOk, ApplyRevertError> {
    let mut progress = RevertProgress::default();
    match git.switch_previous(&target.top) {
        Ok(GitEffect::Applied(())) => {}
        Ok(GitEffect::Rejected(detail)) => {
            return Ok(ApplyRevertOk::Failed { detail, progress });
        }
        Err(source) => return Err(transport("switch to previous branch", progress, source)),
    }
    progress = RevertProgress::BranchSwitched {
        recovery: BranchRecovery::switch_to(&target.onto),
    };

    match git.move_branch(
        &target.top,
        &target.onto,
        &gtl_models::git::GitRevision::from(&target.prior_id),
    ) {
        Ok(GitEffect::Applied(())) => Ok(ApplyRevertOk::Reverted {
            detail: format!(
                "reverted '{}' to {} and switched back",
                target.onto, target.prior_id
            ),
        }),
        Ok(GitEffect::Rejected(detail)) => Ok(ApplyRevertOk::Failed { detail, progress }),
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
    use crate::{
        repositories::{apply_revert, plan_revert::RevertTarget},
        utils::{ScriptedGitClient, branch_name},
    };

    #[test]
    fn successful_revert_reports_the_recovery_position() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("switched\n"),
            ScriptedGitClient::applied(""),
        ]);
        let target = RevertTarget {
            top: crate::utils::repository_root("/repo"),
            onto: branch_name("main"),
            prior_id: crate::utils::commit_id_fixture("abc123"),
        };

        let result =
            apply_revert::execute(target, &git).expect("branch recovery application succeeds");

        assert_eq!(
            result,
            ApplyRevertOk::Reverted {
                detail:
                    "reverted 'main' to abc123abc123abc123abc123abc123abc123abc1 and switched back"
                        .into(),
            }
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_apply_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);
        let target = RevertTarget {
            top: crate::utils::repository_root("/repo"),
            onto: branch_name("main"),
            prior_id: crate::utils::commit_id_fixture("abc123"),
        };

        let error = apply_revert::execute(target, &git)
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
            top: crate::utils::repository_root("/repo"),
            onto: branch_name("main"),
            prior_id: crate::utils::commit_id_fixture("abc123"),
        };

        let result =
            apply_revert::execute(target, &git).expect("a rejected force move is a closed failure");

        let ApplyRevertOk::Failed { progress, .. } = result else {
            panic!("rejected force move must report failure");
        };
        assert_eq!(
            progress,
            RevertProgress::BranchSwitched {
                recovery: BranchRecovery {
                    original_branch: branch_name("main"),
                    command: "git switch main".into(),
                },
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
            top: crate::utils::repository_root("/repo"),
            onto: branch_name("main"),
            prior_id: crate::utils::commit_id_fixture("abc123"),
        };

        let error = apply_revert::execute(target, &git).expect_err("force-move transport fails");
        let ApplyRevertError::Transport {
            progress, source, ..
        } = error;

        assert_eq!(
            progress,
            RevertProgress::BranchSwitched {
                recovery: BranchRecovery {
                    original_branch: branch_name("main"),
                    command: "git switch main".into(),
                },
            }
        );
        assert_eq!(source.to_string(), "branch transport unavailable");
    }
}
