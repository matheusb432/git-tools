use crate::diffs::DiffTarget;

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
