//! Port traits: the seams the application core talks through, implemented by
//! `infra` adapters at the composition root. Every port is `Send + Sync` so a
//! future daemon can share adapters across tokio tasks.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use domain::diffs::{Commit, DiffKind, View};

/// Everything the store needs to record one rendered artifact. `generated_at` is
/// supplied by the caller (via [`Clock`]) so placement stays deterministic in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactMeta {
    pub repo_root: PathBuf,
    pub repo_name: String,
    pub kind: DiffKind,
    pub base_sha: String,
    pub head_sha: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub generated_at: String,
    pub title: String,
}

/// A placed artifact: where it landed and whether an identical one already existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedArtifact {
    pub path: PathBuf,
    pub reused: bool,
}

/// Read-only git access for the diff engine. Every method shells out to git in the
/// real adapter; the fake scripts each return value.
pub trait DiffSource: Send + Sync {
    /// Absolute path of the repository containing `dir`; errors when `dir` is not a git repo.
    fn top_level(&self, dir: &Path) -> anyhow::Result<String>;
    /// The current branch name (`HEAD`'s `--abbrev-ref`).
    fn current_branch(&self, repo: &Path) -> anyhow::Result<String>;
    /// The configured upstream tracking ref; errors when no upstream is configured.
    fn upstream(&self, repo: &Path) -> anyhow::Result<String>;
    /// Confirm `rev` resolves to a commit; errors when it does not.
    fn verify_commit(&self, repo: &Path, rev: &str) -> anyhow::Result<()>;
    /// The abbreviated sha for `rev`.
    fn short_ref(&self, repo: &Path, rev: &str) -> anyhow::Result<String>;
    /// The commits in `range`, newest first.
    fn log_commits(&self, repo: &Path, range: &str) -> anyhow::Result<Vec<Commit>>;
    /// Map each changed path in `range` to the short shas that touched it.
    fn file_commit_map(
        &self,
        repo: &Path,
        range: &str,
    ) -> anyhow::Result<HashMap<String, Vec<String>>>;
    /// The commits `merge` brought into `base..merge` (empty when it introduces nothing).
    fn merge_members(&self, repo: &Path, merge: &str, base: &str) -> anyhow::Result<Vec<String>>;
    /// Raw `git diff` output for `args` (the first arg is the `diff` subcommand).
    fn diff_raw(&self, repo: &Path, args: &[String]) -> anyhow::Result<String>;
    /// Range-bounded forward blame porcelain of `path` at `base..tip`.
    fn blame_forward(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<String>;
    /// Working-tree forward blame porcelain of `path` (hash mode: base → worktree).
    fn blame_forward_worktree(&self, repo: &Path, path: &str) -> anyhow::Result<String>;
    /// Reverse blame porcelain of `path` at `base..tip` (each deleted line carries its deleter).
    fn blame_reverse(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<String>;
    /// The repo's stable oldest root-commit sha, or `None` for a repo with no commits.
    fn root_commit(&self, repo: &Path) -> Option<String>;
    /// Resolve `rev` to its full 40-char sha.
    fn resolve_sha(&self, repo: &Path, rev: &str) -> anyhow::Result<String>;
    /// The committer date of `rev` as a display string; never fails (empty on error).
    fn committed_at(&self, repo: &Path, rev: &str) -> String;
}

/// The content-addressed artifact store behind the diff previews.
pub trait ArtifactStore: Send + Sync {
    /// Place `html` and its metadata under `store_root`, addressed by content hash.
    /// Idempotent per content hash: identical HTML reuses the existing artifact.
    fn place(
        &self,
        store_root: &Path,
        meta: &ArtifactMeta,
        html: &str,
    ) -> anyhow::Result<PlacedArtifact>;
    /// Find an existing artifact for a pure commit range, or `None` on a miss
    /// (always `None` for `WorkTree`, which is never range-addressable).
    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &Path,
        kind: DiffKind,
        base_sha: &str,
        head_sha: &str,
    ) -> anyhow::Result<Option<PathBuf>>;
}

/// Renders a diff [`View`] to a self-contained HTML document.
pub trait HtmlRenderer: Send + Sync {
    /// The complete `file://`-ready HTML for `view`.
    fn build_html(&self, view: &View) -> String;
}

/// A source of the current time as an ISO-8601 string.
pub trait Clock: Send + Sync {
    /// The current instant as a strict ISO-8601 timestamp string.
    fn now_iso(&self) -> String;
}
