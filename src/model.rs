//! Data model for the diff-preview rendering pipeline.
//! Data shapes flowing through the diff-preview rendering pipeline.

/// One changed file: its path, +/- counts, raw diff lines, and the short shas
/// of the commits that touched it (drives the commit filter).
#[derive(Debug, Clone, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    pub lines: Vec<String>,
    pub commits: Vec<String>,
}

/// One commit in range. `date`/`iso` are the human + machine timestamps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub iso: String,
}

/// The `$ <lead><range><trail>` command line shown at the top of the screen.
#[derive(Debug, Clone)]
pub struct Cmd {
    pub lead: String,
    pub range: String,
    pub trail: String,
}

/// The footer prompt line (command + muted note).
#[derive(Debug, Clone)]
pub struct Foot {
    pub cmd: String,
    pub note: String,
}

/// Everything `build_html` needs. In JS these were the destructured params of
/// `buildHtml({...})`, with defaults the caller supplies before rendering.
#[derive(Debug, Clone)]
pub struct View {
    pub repo_name: String,
    /// Absolute path to the repo root (git top-level), used to compose copy-able
    /// absolute file paths in the preview. POSIX-joined with `path` at render time.
    pub repo_root: String,
    pub branch: String,
    pub upstream: String,
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
    pub title: String,
    pub cmd: Cmd,
    pub commits_label: String,
    pub foot: Foot,
}
