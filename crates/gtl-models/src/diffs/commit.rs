mod commit_id;

pub use commit_id::{CommitId, CommitIdAbbreviation, CommitIdError};

/// An immutable Git commit range resolved at invocation time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PinnedRange {
    /// The full commit ID at the exclusive range boundary.
    pub base: CommitId,
    /// The full commit ID at the inclusive range boundary.
    pub head: CommitId,
}

impl PinnedRange {
    /// Returns the exact two-dot range Git computes over.
    pub fn git_range(&self) -> String {
        format!("{}..{}", self.base, self.head)
    }

    /// Returns the stable ten-character presentation of both range endpoints.
    pub fn display_range(&self) -> String {
        format!(
            "{}..{}",
            self.base.abbreviated(CommitIdAbbreviation::TenCharacters),
            self.head.abbreviated(CommitIdAbbreviation::TenCharacters)
        )
    }

    /// Returns the stable ten-character presentation of the range base.
    pub fn display_base(&self) -> String {
        self.base.abbreviated(CommitIdAbbreviation::TenCharacters)
    }
}

/// One commit in a diff range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: CommitId,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub iso: String,
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
            date: String::new(),
            iso: String::new(),
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
