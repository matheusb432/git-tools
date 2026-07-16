//! Applies a planned recovery of a branch's prior position.

use super::{branch_recovery::BranchRecovery, plan_revert::RevertTarget};
use crate::{ports::GitRunner, shared::git::command_label};

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
pub struct RevertResult {
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

impl RevertResult {
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
#[cqrsy::handler(command)]
pub fn execute(
    command: ApplyRevert,
    git: &impl GitRunner,
) -> Result<RevertResult, ApplyRevertError> {
    let ApplyRevert { target } = command;
    let mut progress = RevertProgress::default();
    let switch_args = ["switch", "-"];
    match git.run(&target.top, &switch_args) {
        Ok(output) if output.exit_code == 0 => {}
        Ok(output) => {
            return Ok(RevertResult::new(
                RevertStatus::Failed,
                output.error_line(),
                progress,
            ));
        }
        Err(source) => return Err(transport(&switch_args, progress, source)),
    }
    progress.branch_switched = true;
    progress.recovery = Some(BranchRecovery::switch_to(&target.onto));

    let branch_args = [
        "branch",
        "-f",
        target.onto.as_str(),
        target.prior_sha.as_str(),
    ];
    match git.run(&target.top, &branch_args) {
        Ok(output) if output.exit_code == 0 => {
            progress.force_move_done = true;
            progress.recovery = None;
            Ok(RevertResult::new(
                RevertStatus::Reverted,
                format!(
                    "reverted '{}' to {} and switched back",
                    target.onto, target.prior_sha
                ),
                progress,
            ))
        }
        Ok(output) => Ok(RevertResult::new(
            RevertStatus::Failed,
            output.error_line(),
            progress,
        )),
        Err(source) => Err(transport(&branch_args, progress, source)),
    }
}

fn transport(args: &[&str], progress: RevertProgress, source: anyhow::Error) -> ApplyRevertError {
    ApplyRevertError::Transport {
        command: command_label(args),
        progress,
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{branches::plan_revert::RevertTarget, testing::FakeGitRunner};

    #[test]
    fn successful_revert_reports_the_recovery_position() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok("switched\n"), FakeGitRunner::ok("")]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_sha: "abc123".into(),
        };

        let result =
            execute(ApplyRevert { target }, &git).expect("branch recovery application succeeds");

        assert_eq!(
            result,
            RevertResult {
                status: RevertStatus::Reverted,
                detail: "reverted 'main' to abc123 and switched back".into(),
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
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_sha: "abc123".into(),
        };

        let error = execute(ApplyRevert { target }, &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git switch -: git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }

    #[test]
    fn failed_force_move_reports_switch_and_recovery_command() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("switched\n"),
            FakeGitRunner::exit_err("branch locked", 1),
        ]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_sha: "abc123".into(),
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
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok("switched\n")),
            Err(anyhow::anyhow!("branch transport unavailable")),
        ]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_sha: "abc123".into(),
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
