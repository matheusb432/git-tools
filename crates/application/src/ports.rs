//! Port traits: the seams the application core talks through, implemented by
//! `infra` adapters at the composition root. Every port is cheap to clone,
//! thread-safe, and `'static` so process roots can move adapters across worker
//! boundaries.

use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
};

use domain::{
    diffs::{Commit, DiffExclusions, DiffKind},
    managed::ManagedRepo,
    viewer::RenderOptions,
};

use crate::diffs::View;

/// One effective snapshot of the user configuration used by application operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    theme: Option<String>,
    viewer_render_options: RenderOptions,
    push_confirmation_required: bool,
    diff_exclusions: DiffExclusions,
}

impl AppSettings {
    /// Creates an effective settings snapshot from application and domain values.
    pub fn new(
        theme: Option<String>,
        push_confirmation_required: bool,
        diff_exclusions: DiffExclusions,
    ) -> Self {
        Self {
            theme,
            viewer_render_options: RenderOptions::DEFAULT,
            push_confirmation_required,
            diff_exclusions,
        }
    }

    /// Returns this snapshot with the selected viewer layout and density.
    #[must_use]
    pub const fn with_viewer_render_options(
        mut self,
        viewer_render_options: RenderOptions,
    ) -> Self {
        self.viewer_render_options = viewer_render_options;
        self
    }

    /// Returns the effective viewer layout and density.
    pub const fn viewer_render_options(&self) -> RenderOptions {
        self.viewer_render_options
    }

    /// The raw diff-preview theme, when the renderer should force one.
    pub fn theme(&self) -> Option<&str> {
        self.theme.as_deref()
    }

    /// Whether a plain current-repository push requires confirmation.
    pub const fn push_confirmation_required(&self) -> bool {
        self.push_confirmation_required
    }

    /// The complete project and default diff-exclusion map.
    pub const fn diff_exclusions(&self) -> &DiffExclusions {
        &self.diff_exclusions
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self::new(None, true, DiffExclusions::default())
    }
}

/// Reports a strict user-settings document edit failure.
///
/// # Examples
///
/// ```
/// use application::ports::UserSettingsEditError;
///
/// let error = UserSettingsEditError::InvalidValueShape;
/// assert!(error.to_string().contains("not a string"));
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UserSettingsEditError {
    /// The existing root item is present but is not a string.
    #[error("the existing user setting is not a string")]
    InvalidValueShape,
    /// A path, read, parse, lock, synchronization, or replacement effect failed.
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

pub trait UserSettingsStore: Clone + Send + Sync + 'static {
    /// Loads the current snapshot, degrading adapter failures to safe defaults.
    fn load(&self) -> AppSettings;
}

/// Edits root string values in the configured user-settings document.
///
/// # Examples
///
/// ```no_run
/// use application::ports::{UserSettingsEditError, UserSettingsEditor};
///
/// fn select_light_theme(
///     store: &impl UserSettingsEditor,
/// ) -> Result<Option<String>, UserSettingsEditError> {
///     store.set_string("theme", "light")
/// }
/// ```
pub trait UserSettingsEditor: UserSettingsStore {
    /// Sets one root string and returns the previous string.
    ///
    /// # Errors
    ///
    /// Returns [`UserSettingsEditError`] when the existing item is not a string or
    /// the strict file transaction fails.
    fn set_string(
        &self,
        key: &str,
        value_new: &str,
    ) -> Result<Option<String>, UserSettingsEditError>;

    /// Removes one root string and returns the removed string.
    ///
    /// # Errors
    ///
    /// Returns [`UserSettingsEditError`] when the existing item is not a string or
    /// the strict file transaction fails.
    fn remove_string(&self, key: &str) -> Result<Option<String>, UserSettingsEditError>;
}

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
    /// The configured renderer theme used to build the artifact. `None` means
    /// the renderer selected its default theme.
    pub theme: Option<String>,
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
    /// renderer theme and exclusion set, or `None` on a miss (always `None` for
    /// `WorkTree`, which is never range-addressable).
    #[allow(
        clippy::too_many_arguments,
        reason = "the explicit fields are the persisted range-reuse key"
    )]
    fn lookup_by_range(
        &self,
        store_root: &Path,
        repo_root: &Path,
        kind: DiffKind,
        base_sha: &str,
        head_sha: &str,
        theme: Option<&str>,
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

/// Provides bounded mutable access to one initialized app-state connection.
///
/// Implementations must ensure that every clone shares the same initialized
/// backing connection.
pub trait AppStateStore: Clone + Send + Sync + 'static {
    /// Locks the process-owned connection for one synchronous application operation.
    fn connection_lock(
        &self,
    ) -> anyhow::Result<impl std::ops::DerefMut<Target = rusqlite::Connection> + '_>;
}

/// One captured git invocation: stdout, stderr, and the exit code, exactly as the
/// subprocess reported them. The [`GitRunner`] port's return value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: String,
    /// git's stderr (where it writes diagnostics on failure). Empty unless captured.
    pub stderr: String,
    pub exit_code: i32,
}

impl GitOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    /// git's own diagnostic — stderr (where it writes errors) if present, else stdout —
    /// trimmed. Empty when git said nothing.
    pub fn diagnostic(&self) -> &str {
        let stderr = self.stderr.trim();
        if stderr.is_empty() {
            self.stdout.trim()
        } else {
            stderr
        }
    }

    /// A failure detail: `context` plus git's own message when it gave one, otherwise
    /// `context (exit N)`. Lets every command surface git's real reason uniformly.
    pub fn fail_detail(&self, context: &str) -> String {
        match self.diagnostic() {
            said if !said.is_empty() => format!("{context}: {said}"),
            _ => format!("{context} (exit {})", self.exit_code),
        }
    }

    /// git's own message ([`GitOutput::diagnostic`]) if it gave one, else a generic line.
    /// Shared by every `apply_*` flow (switch, merge, branch, delete), so the fallback
    /// names no specific git subcommand.
    pub fn error_line(&self) -> String {
        match self.diagnostic() {
            said if !said.is_empty() => said.to_string(),
            _ => format!("git command failed (exit {})", self.exit_code),
        }
    }

    /// stdout and stderr joined by a newline — the shape the managed `commit`
    /// fan-out parses for git's own last line (the retired `GitCapture::combined`).
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// The synchronous git-execution seam shared by the local-git command flows (sw,
/// sync, prune, tag, worktree, squash-local, push -r) and the in-process slices
/// behind them. One method mirroring one `git -C <repo> <args…>` invocation; all
/// interpretation of the captured output stays with the caller.
pub trait GitRunner: Clone + Send + Sync + 'static {
    fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput>;

    /// Whether `repo`'s `.git` entry exists on this machine — the sync twin of
    /// [`RemoteSync::repo_present`], and deliberately *not* a git invocation: an
    /// absent repo must classify as absent without spawning a subprocess. The
    /// default is the real fs check; fakes override it to script absence.
    fn repo_present(&self, repo: &Path) -> bool {
        repo.join(".git").exists()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_settings_default_preserves_safe_behavior() {
        let settings = AppSettings::default();

        assert_eq!(settings.theme(), None);
        assert_eq!(settings.viewer_render_options(), RenderOptions::DEFAULT);
        assert!(settings.push_confirmation_required());
        assert!(settings.diff_exclusions().is_empty());
    }

    #[test]
    fn git_output_error_line_prefers_stderr_then_stdout_then_generic() {
        let with_stderr = GitOutput {
            stdout: "ignored".into(),
            stderr: "fatal: boom".into(),
            exit_code: 1,
        };
        assert_eq!(with_stderr.error_line(), "fatal: boom");

        let stdout_only = GitOutput {
            stdout: "some note".into(),
            stderr: String::new(),
            exit_code: 2,
        };
        assert_eq!(stdout_only.error_line(), "some note");

        let silent = GitOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 3,
        };
        assert_eq!(silent.error_line(), "git command failed (exit 3)");
    }
}
