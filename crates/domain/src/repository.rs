//! Structured values describing one repository's local state.

/// Summarizes repository changes shown before a push or commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingChanges {
    /// Counts distinct working-tree paths with any change.
    pub changed: usize,
    /// Counts paths with changes already staged in the index.
    pub staged: usize,
    /// Counts paths with unstaged or untracked changes.
    pub unprepared: usize,
    /// Counts commits ahead of the configured upstream.
    pub ahead: usize,
}
