//! Repository traversal scope and labeled repository targets.

use crate::paths::{ProjectName, RepositoryRoot};

/// Controls whether repository traversal descends into linked worktrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryTraversalScope {
    /// Traverse repositories while pruning linked worktree subtrees below the root.
    ExcludeLinkedWorktrees,
    /// Traverse repositories and linked worktrees.
    IncludeLinkedWorktrees,
}

impl RepositoryTraversalScope {
    /// Returns whether linked worktrees belong to this traversal result.
    #[must_use]
    pub const fn includes_linked_worktrees(self) -> bool {
        matches!(self, Self::IncludeLinkedWorktrees)
    }
}

/// One repository found under a traversal root with its relative display label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryTarget {
    /// The resolved repository top-level directory.
    pub path: RepositoryRoot,
    /// Display label relative to the traversal root, such as `libs/inner`.
    pub label: ProjectName,
}
