//! Applies a planned branch switch.

use super::plan_switch::SwitchTarget;
use crate::{ports::GitRunner, shared::git::command_label};

/// Requests applying one confirmed branch switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySwitch {
    pub target: SwitchTarget,
}

/// Classifies the result of applying a branch switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchStatus {
    Switched,
    Failed,
}

/// Reports the closed branch switch status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchResult {
    pub status: SwitchStatus,
    pub detail: String,
}

impl SwitchResult {
    fn new(status: SwitchStatus, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }
}

/// Reports an unexpected Git transport failure while applying a branch switch.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplySwitchError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Switches to the planned target branch.
///
/// # Errors
///
/// Returns [`ApplySwitchError`] when Git transport fails.
#[cqrsy::command]
pub fn execute(
    command: ApplySwitch,
    git: &impl GitRunner,
) -> Result<SwitchResult, ApplySwitchError> {
    let ApplySwitch { target } = command;
    let args = ["switch", target.onto.as_str()];
    match git.run(&target.top, &args) {
        Ok(output) if output.exit_code == 0 => Ok(SwitchResult::new(
            SwitchStatus::Switched,
            format!("switched to '{}' from '{}'", target.onto, target.from),
        )),
        Ok(output) => Ok(SwitchResult::new(SwitchStatus::Failed, output.error_line())),
        Err(source) => Err(ApplySwitchError::Transport {
            command: command_label(&args),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{branches::plan_switch::SwitchTarget, testing::FakeGitRunner};

    fn target() -> SwitchTarget {
        SwitchTarget {
            top: ".".into(),
            onto: "main".into(),
            from: "feat/x".into(),
        }
    }

    #[test]
    fn successful_switch_reports_the_transition() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok("")]);

        let result =
            execute(ApplySwitch { target: target() }, &git).expect("switch application succeeds");

        assert_eq!(
            result,
            SwitchResult {
                status: SwitchStatus::Switched,
                detail: "switched to 'main' from 'feat/x'".into(),
            }
        );
    }

    #[test]
    fn failed_switch_surfaces_git_error() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::exit_err(
            "error: Your local changes would be overwritten",
            1,
        )]);

        let result = execute(ApplySwitch { target: target() }, &git)
            .expect("a rejected switch is a closed failure");

        assert_eq!(result.status, SwitchStatus::Failed);
        assert!(result.detail.contains("local changes would be overwritten"));
    }

    #[test]
    fn transport_failure_remains_a_sourced_apply_error() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(ApplySwitch { target: target() }, &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(
            error.to_string(),
            "git switch main: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
