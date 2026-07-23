//! Snapshot recipe construction and immutable Git-range pinning.

use crate::diffs::DiffTarget;

pub mod build_managed;
pub mod build_subrepos;
pub mod pin;

/// Identifies the diff-family operation a snapshot recipe performs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeRequest {
    /// Renders a diff target.
    Diff(DiffTarget),
    /// Renders a merge diff against the optional base.
    MergeDiff { base: Option<String> },
    /// Renders the current branch as a squash preview.
    SquashPreview,
}
