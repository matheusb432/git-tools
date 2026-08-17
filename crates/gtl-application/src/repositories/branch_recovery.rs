//! Branch recovery instructions for a partial transition.

use gtl_models::git::BranchName;

/// Identifies the original branch and the exact recovery command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRecovery {
    pub original_branch: BranchName,
    pub command: String,
}

impl BranchRecovery {
    pub(super) fn switch_to(branch: &BranchName) -> Self {
        Self {
            original_branch: branch.clone(),
            command: format!("git switch {branch}"),
        }
    }
}
