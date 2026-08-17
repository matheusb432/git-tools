//! Completed staging and commit steps for current-repository operations.

use gtl_models::diffs::CommitId;

/// Reports the furthest completed local commit step.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CommitProgress {
    /// Neither staging nor commit creation completed.
    #[default]
    Unchanged,
    /// Staging completed, but commit creation did not.
    Staged,
    /// Commit creation completed with its full validated ID.
    Created { id: CommitId },
}
