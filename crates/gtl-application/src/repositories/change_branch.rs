//! Plans and applies one requested repository branch transition.

use std::path::PathBuf;

use gtl_models::git::BranchName;

use super::{
    apply_rebase::{self, ApplyRebase, ApplyRebaseOk},
    apply_revert::{self, ApplyRevert, ApplyRevertOk},
    apply_switch::{self, ApplySwitch, SwitchStatus},
    plan_rebase::{self, PlanRebase, PlanRebaseOk},
    plan_revert::{self, PlanRevert, PlanRevertOk},
    plan_switch::{self, PlanSwitch, PlanSwitchOk},
};
use crate::ports::GitClient;

/// Requests one complete branch transition for a repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeBranch {
    pub repo_path: PathBuf,
    pub onto: BranchName,
    pub action: ChangeBranchAction,
}

/// Selects the branch transition to plan and apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeBranchAction {
    Switch,
    Rebase,
    Revert,
}

/// Reports the closed outcome of a branch transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeBranchOk {
    Switched { detail: String },
    AlreadyThere { detail: String },
    FastForwarded { detail: String },
    Reverted { detail: String },
    NoOp { detail: String },
    Refused { detail: String },
    Failed { detail: String },
}

/// Reports an unexpected Git transport failure while changing a branch.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ChangeBranchError {
    #[error(transparent)]
    PlanSwitch(#[from] plan_switch::PlanSwitchError),
    #[error(transparent)]
    ApplySwitch(#[from] apply_switch::ApplySwitchError),
    #[error(transparent)]
    PlanRebase(#[from] plan_rebase::PlanRebaseError),
    #[error(transparent)]
    ApplyRebase(#[from] apply_rebase::ApplyRebaseError),
    #[error(transparent)]
    PlanRevert(#[from] plan_revert::PlanRevertError),
    #[error(transparent)]
    ApplyRevert(#[from] apply_revert::ApplyRevertError),
}

/// Plans and applies the selected branch transition.
///
/// # Errors
///
/// Returns [`ChangeBranchError`] when Git transport fails during planning or application.
#[cqrsy::command]
pub fn execute(
    command: ChangeBranch,
    git: &impl GitClient,
) -> Result<ChangeBranchOk, ChangeBranchError> {
    let ChangeBranch {
        repo_path,
        onto,
        action,
    } = command;
    match action {
        ChangeBranchAction::Switch => switch(repo_path, onto, git),
        ChangeBranchAction::Rebase => rebase(repo_path, onto, git),
        ChangeBranchAction::Revert => revert(repo_path, onto, git),
    }
}

fn switch(
    repo_path: PathBuf,
    onto: BranchName,
    git: &impl GitClient,
) -> Result<ChangeBranchOk, ChangeBranchError> {
    match plan_switch::execute(PlanSwitch { repo_path, onto }, git)? {
        PlanSwitchOk::Refused(detail) => Ok(ChangeBranchOk::Refused { detail }),
        PlanSwitchOk::AlreadyThere(onto) => Ok(ChangeBranchOk::AlreadyThere {
            detail: format!("already on '{onto}'"),
        }),
        PlanSwitchOk::Ready(target) => {
            let result = apply_switch::execute(ApplySwitch { target }, git)?;
            Ok(match result.status {
                SwitchStatus::Switched => ChangeBranchOk::Switched {
                    detail: result.detail,
                },
                SwitchStatus::Failed => ChangeBranchOk::Failed {
                    detail: result.detail,
                },
            })
        }
    }
}

fn rebase(
    repo_path: PathBuf,
    onto: BranchName,
    git: &impl GitClient,
) -> Result<ChangeBranchOk, ChangeBranchError> {
    match plan_rebase::execute(PlanRebase { repo_path, onto }, git)? {
        PlanRebaseOk::Refused(detail) => Ok(ChangeBranchOk::Refused { detail }),
        PlanRebaseOk::Noop(detail) => Ok(ChangeBranchOk::NoOp { detail }),
        PlanRebaseOk::Ready(target) => {
            let result = apply_rebase::execute(ApplyRebase { target }, git)?;
            Ok(match result {
                ApplyRebaseOk::FastForwarded { detail, .. } => {
                    ChangeBranchOk::FastForwarded { detail }
                }
                ApplyRebaseOk::Failed { detail, .. } => ChangeBranchOk::Failed { detail },
            })
        }
    }
}

fn revert(
    repo_path: PathBuf,
    onto: BranchName,
    git: &impl GitClient,
) -> Result<ChangeBranchOk, ChangeBranchError> {
    match plan_revert::execute(PlanRevert { repo_path, onto }, git)? {
        PlanRevertOk::Refused(detail) => Ok(ChangeBranchOk::Refused { detail }),
        PlanRevertOk::Ready(target) => {
            let result = apply_revert::execute(ApplyRevert { target }, git)?;
            Ok(match result {
                ApplyRevertOk::Reverted { detail } => ChangeBranchOk::Reverted { detail },
                ApplyRevertOk::Failed { detail, .. } => ChangeBranchOk::Failed { detail },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChangeBranch, ChangeBranchAction, ChangeBranchOk};
    use crate::{
        repositories::change_branch,
        utils::{ScriptedGitClient, branch_name},
    };

    fn command(action: ChangeBranchAction) -> ChangeBranch {
        ChangeBranch {
            repo_path: "/repo".into(),
            onto: branch_name("main"),
            action,
        }
    }

    #[test]
    fn switch_plans_and_applies_the_ready_transition() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("feature\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(""),
        ]);

        let result = change_branch::execute(command(ChangeBranchAction::Switch), &git)
            .expect("ready switch should be applied");

        assert_eq!(
            result,
            ChangeBranchOk::Switched {
                detail: "switched to 'main' from 'feature'".into(),
            }
        );
    }

    #[test]
    fn rebase_returns_a_closed_noop_without_applying() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("feature\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("0\n"),
        ]);

        let result = change_branch::execute(command(ChangeBranchAction::Rebase), &git)
            .expect("up-to-date rebase should be a closed outcome");

        assert_eq!(
            result,
            ChangeBranchOk::NoOp {
                detail: "'main' already up to date with 'feature'".into(),
            }
        );
    }

    #[test]
    fn revert_plans_and_applies_the_recovery() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("abc123\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
        ]);

        let result = change_branch::execute(command(ChangeBranchAction::Revert), &git)
            .expect("ready recovery should be applied");

        assert!(matches!(result, ChangeBranchOk::Reverted { .. }));
    }
}
