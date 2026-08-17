//! Structured values reported by Git's worktree registry.

use crate::{diffs::CommitId, git::BranchName, paths::RepositoryRoot};

/// The checked-out state of a non-bare worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeCheckout {
    Branch(BranchName),
    Detached,
}

/// Distinguishes a checkout from a bare repository entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeKind {
    Checkout(WorktreeCheckout),
    Bare,
}

/// Describes one registered Git worktree without independent branch/detached/bare flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    path: RepositoryRoot,
    id: CommitId,
    kind: WorktreeKind,
    locked: Option<String>,
    prunable: Option<String>,
}

impl Worktree {
    pub fn new(
        path: RepositoryRoot,
        id: CommitId,
        kind: WorktreeKind,
        locked: Option<String>,
        prunable: Option<String>,
    ) -> Self {
        Self {
            path,
            id,
            kind,
            locked,
            prunable,
        }
    }

    pub const fn path(&self) -> &RepositoryRoot {
        &self.path
    }

    pub fn into_path(self) -> RepositoryRoot {
        self.path
    }

    pub const fn id(&self) -> &CommitId {
        &self.id
    }

    pub const fn kind(&self) -> &WorktreeKind {
        &self.kind
    }

    pub fn locked(&self) -> Option<&str> {
        self.locked.as_deref()
    }

    pub fn prunable(&self) -> Option<&str> {
        self.prunable.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::{Worktree, WorktreeCheckout, WorktreeKind};
    use crate::{git::BranchName, paths::RepositoryRoot};

    #[test]
    fn checkout_kind_cannot_be_branch_and_detached_or_bare() {
        let worktree = Worktree::new(
            RepositoryRoot::try_new("/repo".into()).unwrap(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .try_into()
                .unwrap(),
            WorktreeKind::Checkout(WorktreeCheckout::Branch(BranchName::main())),
            None,
            None,
        );

        assert!(matches!(
            worktree.kind(),
            WorktreeKind::Checkout(WorktreeCheckout::Branch(branch)) if branch == &BranchName::main()
        ));
    }
}
