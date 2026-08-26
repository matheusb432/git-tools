//! Applies a planned branch switch.

use super::plan_switch::SwitchTarget;
use crate::ports::{GitClient, GitEffect};

/// Classifies the result of applying a branch switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchStatus {
    Switched,
    Failed,
}

/// Reports the closed branch switch status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySwitchOk {
    pub status: SwitchStatus,
    pub detail: String,
}

impl ApplySwitchOk {
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
    target: SwitchTarget,
    git: &impl GitClient,
) -> Result<ApplySwitchOk, ApplySwitchError> {
    let SwitchTarget { top, onto, from } = target;
    match git.switch(&top, &onto) {
        Ok(GitEffect::Applied(())) => Ok(ApplySwitchOk::new(
            SwitchStatus::Switched,
            format!("switched to '{onto}' from '{from}'"),
        )),
        Ok(GitEffect::Rejected(detail)) => Ok(ApplySwitchOk::new(SwitchStatus::Failed, detail)),
        Err(source) => Err(ApplySwitchError::Transport {
            command: format!("switch to {onto}"),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{
        repositories::{apply_switch, plan_switch::SwitchTarget},
        utils::{ScriptedGitClient, branch_name},
    };

    fn target() -> SwitchTarget {
        SwitchTarget {
            top: crate::utils::repository_root("/repo"),
            onto: branch_name("main"),
            from: branch_name("feat/x"),
        }
    }

    #[test]
    fn successful_switch_reports_the_transition() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

        let result = apply_switch::execute(target(), &git).expect("switch application succeeds");

        assert_eq!(
            result,
            ApplySwitchOk {
                status: SwitchStatus::Switched,
                detail: "switched to 'main' from 'feat/x'".into(),
            }
        );
    }

    #[test]
    fn failed_switch_surfaces_git_error() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::rejected(
            "error: Your local changes would be overwritten",
        )]);

        let result =
            apply_switch::execute(target(), &git).expect("a rejected switch is a closed failure");

        assert_eq!(result.status, SwitchStatus::Failed);
        assert!(result.detail.contains("local changes would be overwritten"));
    }

    #[test]
    fn transport_failure_remains_a_sourced_apply_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = apply_switch::execute(target(), &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(
            error.to_string(),
            "switch to main: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
