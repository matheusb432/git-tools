//! Port traits: the seams the application core talks through, implemented by
//! `infra` adapters at the composition root. Every port is a cheap-clone,
//! thread-safe `'static` handle so cqrsy can prepare operation dependencies and
//! process roots can move adapters across worker boundaries.

use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
};

use domain::{
    diffs::{Commit, DiffKind},
    managed::ManagedRepo,
    viewer::RenderHistoryId,
};

use crate::diffs::View;

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
    /// The extension set that was in force when the artifact rendered (normalized,
    /// sorted; empty = unfiltered). Part of the range-reuse key: an artifact is
    /// only reusable by a render running under the same filter.
    pub excluded_extensions: Vec<String>,
}

/// A placed artifact: where it landed and whether an identical one already existed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedArtifact {
    pub path: PathBuf,
    pub reused: bool,
}

/// One row from the content-addressed store's history listing — the port-facing
/// mirror of infra's private `Sidecar`, same reasoning as `ArtifactMeta`/`place`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    pub repo_id: String,
    pub repo_name: String,
    pub title: String,
    pub range_label: String,
    pub head_committed_at: String,
    pub generated_at: String,
    pub content_hash: String,
    pub kind: DiffKind,
    pub byte_size: u64,
}

/// One saved live view, keyed by its adapter-shaped source identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewRecord {
    pub source_kind: String,
    pub source_value: String,
    pub display_name: String,
    pub created_at: String,
    pub last_opened_at: Option<String>,
}

/// One new render recipe to append to app history, never a diff payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRecentRenderRecord {
    pub recipe_json: String,
    pub title: String,
    pub repo_name: String,
    pub kind: String,
    pub range_label: String,
    pub rendered_at: String,
}

/// One persisted render recipe read from app history with its stable row identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRenderRecord {
    pub id: RenderHistoryId,
    pub recipe_json: String,
    pub title: String,
    pub repo_name: String,
    pub kind: String,
    pub range_label: String,
    pub rendered_at: String,
}

/// Describes typed failures surfaced by app-state persistence adapters.
#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    /// Reports a persisted recent-render row whose identity violates the domain invariant.
    #[error("recent_renders row id {id} violates the positive-ID invariant")]
    InvalidRecentRenderId { id: i64 },
    /// Reports an unexpected storage or migration failure.
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Cap on retained `recent_renders` rows; the adapter prunes past it on insert.
pub const RECENT_RENDERS_CAP: usize = 500;

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

/// Read-only git access for the diff engine. Every method shells out to git in the
/// real adapter; the fake scripts each return value.
pub trait DiffSource: Clone + Send + Sync + 'static {
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
pub trait ArtifactStore: Clone + Send + Sync + 'static {
    /// Place `html` and its metadata under `store_root`, addressed by content hash.
    /// Idempotent per content hash: identical HTML reuses the existing artifact.
    fn place(
        &self,
        store_root: &Path,
        meta: &ArtifactMeta,
        html: &str,
    ) -> anyhow::Result<PlacedArtifact>;
    /// Find an existing artifact for a pure commit range rendered under the same
    /// exclusion set, or `None` on a miss (always `None` for `WorkTree`, which is
    /// never range-addressable).
    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &Path,
        kind: DiffKind,
        base_sha: &str,
        head_sha: &str,
        excluded_extensions: &[String],
    ) -> anyhow::Result<Option<PathBuf>>;
    /// Every recorded artifact under `store_root`, across all repos, unordered
    /// (the `history/list` handler owns sort order).
    fn list_history(&self, store_root: &Path) -> anyhow::Result<Vec<HistoryRecord>>;
}

/// Renders a diff [`View`] to a self-contained HTML document.
pub trait HtmlRenderer: Clone + Send + Sync + 'static {
    /// The complete `file://`-ready HTML for `view`.
    fn build_html(&self, view: &View) -> String;
    /// Renders several views as one tab-stripped document (diff-subrepos / diff --all).
    fn build_tabbed_html(&self, title: &str, views: &[View]) -> String;
}

/// A source of the current time as an ISO-8601 string.
pub trait Clock: Clone + Send + Sync + 'static {
    /// The current instant as a strict ISO-8601 timestamp string.
    fn now_iso(&self) -> String;
}

/// Filesystem/git probe behind live-view validation.
pub trait RepoProbe: Clone + Send + Sync + 'static {
    /// Classify `dir`; errors only on unexpected I/O failures, never on the
    /// three expected outcomes (those are values).
    fn probe(&self, dir: &Path) -> anyhow::Result<RepoProbeResult>;
}

/// Filesystem discovery of git repos under a root. The walk — and its prune/skip
/// decisions, via `crate::discovery::rules` — lives in the infra adapter; the
/// `discovery::find_repos` slice labels the results.
pub trait RepoDiscovery: Clone + Send + Sync + 'static {
    /// Every git repo directory under `root`, sorted. Linked worktrees (and their
    /// subtrees) are skipped unless `include_worktrees`; VCS-internal and
    /// build/dependency directories are always pruned.
    fn find_repos(&self, root: &Path, include_worktrees: bool) -> anyhow::Result<Vec<PathBuf>>;
}

/// The app-state store: saved live views, settings, and the recent-render log.
/// One `SQLite` file under `data_root` in the real adapter; every method opens,
/// migrates, and closes per call so callers stay hermetic (the `ArtifactStore`
/// per-call `store_root` pattern).
pub trait AppStateStore: Clone + Send + Sync + 'static {
    /// Upsert by `(source_kind, source_value)`: a new row keeps `record` as-is;
    /// an existing row keeps its `created_at`/`last_opened_at` and takes only
    /// the new `display_name`. Returns `true` when the view already existed.
    fn save_live_view(&self, data_root: &Path, record: &LiveViewRecord) -> anyhow::Result<bool>;
    /// Every saved live view, oldest first (creation order).
    fn list_live_views(&self, data_root: &Path) -> anyhow::Result<Vec<LiveViewRecord>>;
    /// Delete by identity; `true` when a row was actually removed.
    fn remove_live_view(
        &self,
        data_root: &Path,
        source_kind: &str,
        source_value: &str,
    ) -> anyhow::Result<bool>;
    /// The stored value for `key`, or `None` when unset.
    fn get_setting(&self, data_root: &Path, key: &str) -> anyhow::Result<Option<String>>;
    /// Upsert `key` to `value`.
    fn set_setting(&self, data_root: &Path, key: &str, value: &str) -> anyhow::Result<()>;
    /// Append one render record, pruning the log past [`RECENT_RENDERS_CAP`].
    fn record_render(&self, data_root: &Path, record: &NewRecentRenderRecord)
    -> anyhow::Result<()>;
    /// Recorded renders, newest first.
    fn list_recent_renders(
        &self,
        data_root: &Path,
    ) -> Result<Vec<RecentRenderRecord>, AppStateError>;
    /// The recorded render identified by `id`, or `None` when no such row exists.
    fn get_recent_render(
        &self,
        data_root: &Path,
        id: RenderHistoryId,
    ) -> Result<Option<RecentRenderRecord>, AppStateError>;
}

/// One captured git subprocess result: exit success plus combined stdout+stderr.
/// Mirrors the CLI's retired `managed::git_capture::GitCapture::{success, combined}`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncOutput {
    pub success: bool,
    pub combined: String,
}

/// Async git remote operations behind `push --all`/`pull --all`. Every method mirrors exactly
/// one git invocation the CLI's retired `push_pull.rs` shelled out to; fallback rules
/// (empty branch on failure, treat a rev-list error as zero, etc.) stay in the
/// `managed` slices, not here — same split as [`DiffSource`].
pub trait RemoteSync: Clone + Send + Sync + 'static {
    /// Whether `repo`'s `.git` directory exists on this machine (pure fs check).
    fn repo_present(&self, repo: &Path) -> bool;
    /// `git rev-parse --abbrev-ref HEAD`.
    fn current_branch(&self, repo: &Path) -> impl Future<Output = anyhow::Result<String>> + Send;
    /// `git remote get-url <remote>`; `Ok(true)` iff it resolves.
    fn has_remote(
        &self,
        repo: &Path,
        remote: &str,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
    /// `git push <remote> <branch> [--dry-run]`.
    fn push(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        dry: bool,
    ) -> impl Future<Output = anyhow::Result<SyncOutput>> + Send;
    /// `git fetch <remote>`.
    fn fetch(
        &self,
        repo: &Path,
        remote: &str,
    ) -> impl Future<Output = anyhow::Result<SyncOutput>> + Send;
    /// `git rev-parse --abbrev-ref --symbolic-full-name @{u}`; `Ok(None)` if no upstream.
    fn upstream_ref(
        &self,
        repo: &Path,
    ) -> impl Future<Output = anyhow::Result<Option<String>>> + Send;
    /// `git rev-parse --verify --quiet <remote_ref>`; `Ok(true)` iff it resolves.
    fn verify_ref(
        &self,
        repo: &Path,
        remote_ref: &str,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
    /// `git rev-list --count <range>`.
    fn rev_list_count(
        &self,
        repo: &Path,
        range: &str,
    ) -> impl Future<Output = anyhow::Result<usize>> + Send;
    /// `git rev-list --count --left-right <range>` -> `(behind, ahead)`.
    fn rev_list_left_right(
        &self,
        repo: &Path,
        range: &str,
    ) -> impl Future<Output = anyhow::Result<(usize, usize)>> + Send;
    /// `git merge --ff-only <remote_ref>`.
    fn merge_ff_only(
        &self,
        repo: &Path,
        remote_ref: &str,
    ) -> impl Future<Output = anyhow::Result<SyncOutput>> + Send;
}

/// Parses an already-resolved manifest file into the managed repo list. *Where*
/// the file lives (env var, upward search, `sample_project`, home-dir default) stays a
/// CLI concern — it depends on the caller's shell cwd, same reasoning as
/// merge-diff's cwd-absolute-resolution rule.
pub trait ManagedManifest: Clone + Send + Sync + 'static {
    fn load(
        &self,
        repos_file: &Path,
        home_dir: &Path,
    ) -> impl Future<Output = anyhow::Result<Vec<ManagedRepo>>> + Send;
}

/// One recorded push-ledger fact: the ahead-count observed and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    pub ahead: usize,
    pub checked_at: String,
}

/// The push-ledger seam. No real storage yet — `record`/`refresh` are no-ops and
/// `last_known` always answers `None` behind the shipped adapter; a future effort
/// gives this a real backing store and teaches `push_all` to consult `last_known`
/// to skip already-synced repos without a network round trip.
pub trait PushLedger: Clone + Send + Sync + 'static {
    fn record(
        &self,
        repo_name: &str,
        ahead: usize,
        checked_at: &str,
    ) -> impl Future<Output = ()> + Send;
    fn last_known(&self, repo_name: &str) -> impl Future<Output = Option<LedgerEntry>> + Send;
    /// Periodic refresh hook the daemon's background worker calls.
    fn refresh(&self) -> impl Future<Output = ()> + Send;
}
