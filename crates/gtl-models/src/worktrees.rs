//! Structured values reported by Git's worktree registry.

use crate::diffs::CommitId;

/// Describes one registered Git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Absolute or caller-facing path reported by Git.
    pub path: String,
    /// Full commit ID at the worktree's `HEAD`.
    pub id: CommitId,
    /// Local branch name without the `refs/heads/` prefix.
    pub branch: Option<String>,
    /// Whether Git reports a detached `HEAD`.
    pub detached: bool,
    /// Whether Git reports a bare worktree.
    pub bare: bool,
    /// Optional reason Git reports the worktree as locked.
    pub locked: Option<String>,
    /// Optional reason Git reports the worktree as prunable.
    pub prunable: Option<String>,
}
