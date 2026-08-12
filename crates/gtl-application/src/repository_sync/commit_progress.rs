//! Completed staging and commit steps for current-repository operations.

/// Reports the furthest completed local commit step.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CommitProgress {
    /// Neither staging nor commit creation completed.
    #[default]
    Unchanged,
    /// Staging completed, but commit creation did not.
    Staged,
    /// Commit creation completed, with its parsed identity when Git reported one.
    Created { identity: Option<String> },
}
