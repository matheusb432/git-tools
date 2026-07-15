use std::collections::HashMap;

/// One changed file: its path, +/- counts, raw diff lines, the short shas of the
/// commits that touched it (drives the file filter), and per-line ownership
/// (drives the per-commit line highlight).
#[derive(Debug, Clone, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    pub lines: Vec<String>,
    pub full_lines: Option<Vec<String>>,
    pub commits: Vec<String>,
    pub owners: LineOwners,
}

/// Per-line commit ownership for one file, keyed by absolute line number:
/// `added` by new-side line, `deleted` by old-side line. The same maps serve
/// both the compact and full-file panes — changed rows keep absolute numbers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineOwners {
    pub added: HashMap<u32, String>,
    pub deleted: HashMap<u32, String>,
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
