//! Closed working-tree state shared by repository status and project commit operations.

use nutype::nutype;
use serde::Serialize;

use crate::paths::RepositoryRelativePath;

/// One changed working-tree file: its porcelain status code and reported path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CommitFile {
    pub status: String,
    pub path: RepositoryRelativePath,
}

impl CommitFile {
    /// Returns whether Git classified the path as untracked.
    #[must_use]
    pub fn is_untracked(&self) -> bool {
        self.status == "??"
    }
}

/// A non-empty collection of changed working-tree files.
#[nutype(
    validate(predicate = |files| !files.is_empty()),
    derive(Debug, Clone, PartialEq, Eq, Deref, Serialize)
)]
pub struct ChangedFiles(Vec<CommitFile>);

/// The complete repository working-tree state known to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirtyState {
    /// The repository is not present on this machine.
    Absent,
    /// Git confirmed an empty working tree.
    Clean,
    /// Git reported one or more changed files.
    Dirty(ChangedFiles),
    /// The repository exists, but Git rejected the status query.
    Unavailable { detail: String },
}

impl DirtyState {
    /// Builds a clean or dirty state from a successful Git status response.
    pub fn from_files(files: Vec<CommitFile>) -> Self {
        ChangedFiles::try_new(files).map_or(Self::Clean, Self::Dirty)
    }

    #[must_use]
    pub fn files(&self) -> &[CommitFile] {
        match self {
            Self::Dirty(files) => files,
            Self::Absent | Self::Clean | Self::Unavailable { .. } => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ChangedFiles, CommitFile, DirtyState, RepositoryRelativePath};

    fn changed_file(status: &str) -> CommitFile {
        CommitFile {
            status: status.into(),
            path: RepositoryRelativePath::try_new("src/lib.rs".into()).unwrap(),
        }
    }

    #[test]
    fn successful_file_reads_cannot_build_an_empty_dirty_state() {
        assert_eq!(DirtyState::from_files(Vec::new()), DirtyState::Clean);
        assert!(ChangedFiles::try_new(Vec::new()).is_err());

        let file = changed_file("M");
        assert_eq!(
            DirtyState::from_files(vec![file.clone()]),
            DirtyState::Dirty(ChangedFiles::try_new(vec![file]).unwrap(),)
        );
    }

    #[test]
    fn changed_files_serialize_as_their_file_sequence() {
        let files = ChangedFiles::try_new(vec![changed_file("M")]).unwrap();
        let serialized = serde_json::to_value(files).unwrap();

        assert_eq!(
            serialized,
            json!([{
                "Status": "M",
                "Path": "src/lib.rs",
            }])
        );
    }

    #[test]
    fn untracked_classification_is_owned_by_the_porcelain_file_value() {
        assert!(changed_file("??").is_untracked());
        assert!(!changed_file("M").is_untracked());
    }
}
