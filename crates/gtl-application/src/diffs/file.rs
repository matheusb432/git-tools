use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};

use super::source_lines::DiffSourceLines;

/// One changed file with its path, line counts, and raw diff lines.
#[derive(Debug, Clone, PartialEq)]
pub struct FileDiff {
    pub path: RepositoryRelativePath,
    pub added: DiffLineCount,
    pub removed: DiffLineCount,
    pub lines: DiffSourceLines,
    pub full_lines: Option<DiffSourceLines>,
}

/// The change kind encoded by a file's raw Git diff metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Renamed,
    Modified,
}

impl FileDiff {
    /// Classifies the file from Git's raw diff metadata.
    #[must_use]
    pub fn status(&self) -> FileStatus {
        self.lines.status()
    }
}
