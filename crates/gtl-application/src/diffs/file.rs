use gtl_models::{diffs::DiffLineCount, paths::RepositoryRelativePath};

/// One changed file with its path, line counts, and raw diff lines.
#[derive(Debug, Clone, PartialEq)]
pub struct FileDiff {
    pub path: RepositoryRelativePath,
    pub added: DiffLineCount,
    pub removed: DiffLineCount,
    pub lines: Vec<String>,
    pub full_lines: Option<Vec<String>>,
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
        if self
            .lines
            .iter()
            .any(|line| line.starts_with("rename from ") || line.starts_with("rename to "))
        {
            return FileStatus::Renamed;
        }
        if self
            .lines
            .iter()
            .any(|line| line.starts_with("new file ") || line == "--- /dev/null")
        {
            return FileStatus::Added;
        }
        if self
            .lines
            .iter()
            .any(|line| line.starts_with("deleted file ") || line == "+++ /dev/null")
        {
            return FileStatus::Deleted;
        }
        FileStatus::Modified
    }
}
