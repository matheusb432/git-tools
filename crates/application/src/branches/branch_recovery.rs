//! Describes how to return to the branch active before a partial transition.

/// Identifies the original branch and the exact recovery command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRecovery {
    pub original_branch: String,
    pub command: String,
}

impl BranchRecovery {
    pub(super) fn switch_to(branch: &str) -> Self {
        Self {
            original_branch: branch.to_string(),
            command: format!("git switch {branch}"),
        }
    }
}
