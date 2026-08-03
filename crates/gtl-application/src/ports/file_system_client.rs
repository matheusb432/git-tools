use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemEntryKind {
    File,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemClientErrorKind {
    NotFound,
    Other,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct FileSystemClientError {
    kind: FileSystemClientErrorKind,
    message: String,
}

impl FileSystemClientError {
    pub fn new(kind: FileSystemClientErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> FileSystemClientErrorKind {
        self.kind
    }
}

pub trait FileSystemClient: Clone + Send + Sync + 'static {
    fn canonical_working_directory(&self) -> Result<PathBuf, FileSystemClientError>;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FileSystemClientError>;
    fn entry_kind(&self, path: &Path) -> Result<FileSystemEntryKind, FileSystemClientError>;
}
