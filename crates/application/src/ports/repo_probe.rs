use std::path::{Path, PathBuf};

/// What probing a directory for a git repository found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoProbeResult {
    /// A repo, with its canonical top-level path.
    Repo { top_level: PathBuf },
    /// The directory does not exist.
    NotFound,
    /// The directory exists but is not inside a git work tree.
    NotAGitRepo,
}

/// Filesystem/git probe behind live-view validation.
pub trait RepoProbe: Clone + Send + Sync + 'static {
    /// Classify `dir`; errors only on unexpected I/O failures, never on the
    /// three expected outcomes (those are values).
    fn probe(&self, dir: &Path) -> anyhow::Result<RepoProbeResult>;
}
