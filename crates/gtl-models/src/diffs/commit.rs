mod commit_id;

pub use commit_id::{CommitId, CommitIdAbbreviation, CommitIdError};

use crate::{
    git::{GitRange, GitRevision},
    timestamps::MachineTimestamp,
};

/// An immutable Git commit range resolved at invocation time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PinnedRange {
    /// The full commit ID at the exclusive range boundary.
    pub base: CommitId,
    /// The full commit ID at the inclusive range boundary.
    pub head: CommitId,
}

impl PinnedRange {
    /// Returns the exact two-dot range Git computes over as a validated value.
    pub fn to_git_range(&self) -> GitRange {
        GitRange::two_dot(
            &GitRevision::from(&self.base),
            &GitRevision::from(&self.head),
        )
    }

    /// Returns the exact two-dot range Git computes over.
    pub fn git_range(&self) -> String {
        self.to_git_range().to_string()
    }

    /// Returns the stable presentation range as a validated value.
    pub fn to_display_range(&self) -> GitRange {
        GitRange::two_dot(
            &self.to_display_base(),
            &GitRevision::abbreviated_commit(&self.head, CommitIdAbbreviation::TenCharacters),
        )
    }

    /// Returns the stable ten-character presentation of both range endpoints.
    pub fn display_range(&self) -> String {
        self.to_display_range().to_string()
    }

    /// Returns the stable presentation of the range base as a validated revision.
    pub fn to_display_base(&self) -> GitRevision {
        GitRevision::abbreviated_commit(&self.base, CommitIdAbbreviation::TenCharacters)
    }

    /// Returns the stable ten-character presentation of the range base.
    pub fn display_base(&self) -> String {
        self.to_display_base().to_string()
    }
}

/// One commit in a diff range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: CommitId,
    pub subject: String,
    pub body: String,
    pub committed_at: MachineTimestamp,
    pub parents: Vec<CommitId>,
}

impl Commit {
    /// A commit with two or more parents is a merge.
    pub fn is_merge(&self) -> bool {
        self.parents.len() >= 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit_id(digit: char) -> CommitId {
        digit
            .to_string()
            .repeat(40)
            .try_into()
            .expect("fixture commit ID should be valid")
    }

    fn commit() -> Commit {
        Commit {
            id: commit_id('0'),
            subject: String::new(),
            body: String::new(),
            committed_at: MachineTimestamp::try_from("2026-01-01T00:00:00Z")
                .expect("fixture commit timestamp is valid"),
            parents: Vec::new(),
        }
    }

    #[test]
    fn is_merge_is_true_only_with_two_or_more_parents() {
        let mut commit = commit();

        assert!(!commit.is_merge());
        commit.parents.push(commit_id('1'));
        assert!(!commit.is_merge());
        commit.parents.push(commit_id('2'));
        assert!(commit.is_merge());
    }
}
