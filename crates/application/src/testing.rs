//! In-memory fakes for the application ports (template pattern: cheap-clone
//! shared state so tests keep a handle after passing a fake in).

#[cfg(test)]
pub(crate) mod diffs;

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use domain::{
    diffs::{Commit, DiffKind},
    managed::ManagedRepo,
    viewer::RenderOptions,
};

#[cfg(test)]
use crate::ports::AppStateStore;
use crate::ports::{
    AppSettings, ArtifactMeta, ArtifactStore, Clock, DiffSource, GitOutput, GitRunner,
    HistoryRecord, HtmlRenderer, LedgerEntry, ManagedManifest, PlacedArtifact, PushLedger,
    RemoteSync, RepoDiscovery, RepoProbe, RepoProbeResult, SyncOutput, UserSettingsStore,
};

/// Fixed effective settings for application operation tests.
#[derive(Debug, Clone, Default)]
pub struct FixedUserSettingsStore {
    settings: AppSettings,
}

impl FixedUserSettingsStore {
    /// Creates a store that returns `settings` from every load.
    pub const fn new(settings: AppSettings) -> Self {
        Self { settings }
    }
}

impl UserSettingsStore for FixedUserSettingsStore {
    fn load(&self) -> AppSettings {
        self.settings.clone()
    }
}

/// Scripted `DiffSource`: every field is what the corresponding method returns.
#[derive(Debug, Default, Clone)]
pub struct FakeDiffSource {
    pub top_level: Option<String>,
    pub branch: String,
    pub upstream: Option<String>,
    pub commits: Vec<Commit>,
    pub file_commits: HashMap<String, Vec<String>>,
    pub diff_output: String,
    pub full_diff_output: String,
    pub known_revs: Vec<String>,
    pub shas: HashMap<String, String>,
    pub committed_at: String,
    /// Per-repo overrides for `commits`/`diff_output`, keyed by the `top` path
    /// `build_view` is called with -- lets one scripted source produce different
    /// results (e.g. one repo empty, one not) across a single `render_batch` call,
    /// which otherwise can only script one outcome for every repo.
    pub per_repo: HashMap<String, RepoOverride>,
}

/// See [`FakeDiffSource::per_repo`].
#[derive(Debug, Default, Clone)]
pub struct RepoOverride {
    pub commits: Vec<Commit>,
    pub diff_output: String,
}

impl FakeDiffSource {
    /// The scripted primary diff for `repo`: its per-repo override, else the shared one.
    fn scripted_diff(&self, repo: &Path) -> String {
        self.per_repo
            .get(&repo.to_string_lossy().into_owned())
            .map_or_else(|| self.diff_output.clone(), |o| o.diff_output.clone())
    }
}

impl DiffSource for FakeDiffSource {
    fn top_level(&self, _dir: &Path) -> anyhow::Result<String> {
        self.top_level
            .clone()
            .ok_or_else(|| anyhow::anyhow!("not a git repository"))
    }
    fn current_branch(&self, _repo: &Path) -> anyhow::Result<String> {
        Ok(self.branch.clone())
    }
    fn upstream(&self, _repo: &Path) -> anyhow::Result<String> {
        self.upstream
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no upstream"))
    }
    fn verify_commit(&self, _repo: &Path, rev: &str) -> anyhow::Result<()> {
        if self.known_revs.iter().any(|r| r == rev) {
            Ok(())
        } else {
            anyhow::bail!("unknown revision {rev}")
        }
    }
    fn short_ref(&self, _repo: &Path, rev: &str) -> anyhow::Result<String> {
        Ok(rev.to_string())
    }
    fn log_commits(&self, repo: &Path, _range: &str) -> anyhow::Result<Vec<Commit>> {
        Ok(self
            .per_repo
            .get(&repo.to_string_lossy().into_owned())
            .map_or_else(|| self.commits.clone(), |o| o.commits.clone()))
    }
    fn file_commit_map(
        &self,
        _repo: &Path,
        _range: &str,
    ) -> anyhow::Result<HashMap<String, Vec<String>>> {
        Ok(self.file_commits.clone())
    }
    fn merge_members(
        &self,
        _repo: &Path,
        _merge: &str,
        _base: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn diff_raw(&self, repo: &Path, args: &[String]) -> anyhow::Result<String> {
        // The exclusion pass asks for paths only; mirror git by listing the
        // scripted diff's file paths, one per line.
        if args.iter().any(|a| a == "--name-only") {
            let paths: Vec<String> = crate::diffs::util::parse_diff(&self.scripted_diff(repo))
                .into_iter()
                .map(|file| file.path)
                .collect();
            return Ok(paths.join("\n"));
        }
        // Full-context re-runs carry the -U flag added by `full_context_args`.
        if args
            .iter()
            .any(|a| a.starts_with("--unified") || a.starts_with("-U"))
        {
            return Ok(self.full_diff_output.clone());
        }
        Ok(self.scripted_diff(repo))
    }
    fn blame_forward(&self, _r: &Path, _b: &str, _t: &str, _p: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
    fn blame_forward_worktree(&self, _r: &Path, _p: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
    fn blame_reverse(&self, _r: &Path, _b: &str, _t: &str, _p: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
    fn root_commit(&self, _repo: &Path) -> Option<String> {
        Some("rootsha".into())
    }
    fn resolve_sha(&self, _repo: &Path, rev: &str) -> anyhow::Result<String> {
        Ok(self
            .shas
            .get(rev)
            .cloned()
            .unwrap_or_else(|| format!("sha-{rev}")))
    }
    fn committed_at(&self, _repo: &Path, _rev: &str) -> String {
        self.committed_at.clone()
    }
}

/// In-memory artifact store with deterministic paths and scripted range hits.
/// Scripted range-lookup hits keyed by range, layout, density, theme, and exclusions.
pub type RangeHits = Arc<
    Mutex<
        HashMap<
            (
                DiffKind,
                String,
                String,
                RenderOptions,
                Option<String>,
                Vec<String>,
            ),
            PathBuf,
        >,
    >,
>;

/// Artifact content observable through [`InMemoryArtifactStore`].
#[derive(Debug, Clone, PartialEq)]
pub struct StoredArtifact {
    /// Metadata stored alongside the rendered document.
    pub meta: ArtifactMeta,
    /// Rendered document content.
    pub html: String,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryArtifactStore {
    /// Persisted artifacts keyed by their deterministic path.
    pub artifacts: Arc<Mutex<HashMap<PathBuf, StoredArtifact>>>,
    pub range_hits: RangeHits,
    pub history: Vec<HistoryRecord>,
}

impl InMemoryArtifactStore {
    /// Returns the persisted artifact at `path`.
    pub fn artifact(&self, path: &Path) -> Option<StoredArtifact> {
        self.artifacts.lock().ok()?.get(path).cloned()
    }

    #[cfg(test)]
    #[allow(
        clippy::too_many_arguments,
        reason = "the explicit fields are the persisted range-reuse key"
    )]
    pub(crate) fn range_hit_insert(
        &self,
        kind: DiffKind,
        base_sha: &str,
        head_sha: &str,
        render_options: RenderOptions,
        theme: Option<&str>,
        excluded_extensions: &[&str],
        artifact_path: &str,
    ) {
        self.range_hits.lock().unwrap().insert(
            (
                kind,
                base_sha.to_owned(),
                head_sha.to_owned(),
                render_options,
                theme.map(str::to_owned),
                excluded_extensions
                    .iter()
                    .map(|extension| (*extension).to_owned())
                    .collect(),
            ),
            PathBuf::from(artifact_path),
        );
    }
}

impl ArtifactStore for InMemoryArtifactStore {
    fn place(
        &self,
        store_root: &Path,
        meta: &ArtifactMeta,
        html: &str,
    ) -> anyhow::Result<PlacedArtifact> {
        let path = store_root.join("diffs/fake/artifact.html");
        self.artifacts.lock().unwrap().insert(
            path.clone(),
            StoredArtifact {
                meta: meta.clone(),
                html: html.to_string(),
            },
        );
        Ok(PlacedArtifact {
            path,
            reused: false,
        })
    }
    fn lookup_by_range(
        &self,
        _store_root: &Path,
        _repo_root: &Path,
        kind: DiffKind,
        base_sha: &str,
        head_sha: &str,
        render_options: RenderOptions,
        theme: Option<&str>,
        excluded_extensions: &[String],
    ) -> anyhow::Result<Option<PathBuf>> {
        Ok(self
            .range_hits
            .lock()
            .unwrap()
            .get(&(
                kind,
                base_sha.to_string(),
                head_sha.to_string(),
                render_options,
                theme.map(str::to_owned),
                excluded_extensions.to_vec(),
            ))
            .cloned())
    }
    fn list_history(&self, _store_root: &Path) -> anyhow::Result<Vec<HistoryRecord>> {
        Ok(self.history.clone())
    }
}

/// Renderer returning a canned document (content-independent).
#[derive(Debug, Default, Clone)]
pub struct StubRenderer;

impl HtmlRenderer for StubRenderer {
    fn build_html(
        &self,
        view: &crate::diffs::View,
        options: RenderOptions,
        theme: Option<&str>,
    ) -> String {
        format!(
            "<html data-theme=\"{}\" data-layout=\"{}\" data-density=\"{}\"><title>{}</title></html>",
            theme.unwrap_or_default(),
            options.layout(),
            options.density(),
            view.title
        )
    }
    fn build_tabbed_html(
        &self,
        title: &str,
        views: &[crate::diffs::View],
        options: RenderOptions,
        theme: Option<&str>,
    ) -> String {
        let view_summaries = views
            .iter()
            .map(|view| {
                format!(
                    "{}:{}:{}:{}:{}",
                    view.repo_name,
                    theme.unwrap_or_default(),
                    options.layout(),
                    options.density(),
                    view.files.len()
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        format!("<html><title>{title}</title>{view_summaries}</html>")
    }
}

/// Clock pinned to a fixed instant.
#[derive(Debug, Clone)]
pub struct FixedClock(pub String);

impl Clock for FixedClock {
    fn now_iso(&self) -> String {
        self.0.clone()
    }
}

/// Scripted `RemoteSync`: every field is what the corresponding method returns.
/// Single-scripted (no per-repo map) — every existing push/pull test scripts one
/// repo's worth of git responses, matching `push_pull.rs`'s existing test style.
#[derive(Debug, Default, Clone)]
pub struct FakeRemoteSync {
    pub present: bool,
    pub branch: String,
    pub has_remote: bool,
    pub push_result: SyncOutput,
    pub fetch_result: SyncOutput,
    pub upstream: Option<String>,
    pub verify_ref: bool,
    pub rev_list_count: usize,
    pub rev_list_left_right: (usize, usize),
    pub merge_result: SyncOutput,
}

impl RemoteSync for FakeRemoteSync {
    fn repo_present(&self, _repo: &Path) -> bool {
        self.present
    }
    async fn current_branch(&self, _repo: &Path) -> anyhow::Result<String> {
        Ok(self.branch.clone())
    }
    async fn has_remote(&self, _repo: &Path, _remote: &str) -> anyhow::Result<bool> {
        Ok(self.has_remote)
    }
    async fn push(
        &self,
        _repo: &Path,
        _remote: &str,
        _branch: &str,
        _dry: bool,
    ) -> anyhow::Result<SyncOutput> {
        Ok(self.push_result.clone())
    }
    async fn fetch(&self, _repo: &Path, _remote: &str) -> anyhow::Result<SyncOutput> {
        Ok(self.fetch_result.clone())
    }
    async fn upstream_ref(&self, _repo: &Path) -> anyhow::Result<Option<String>> {
        Ok(self.upstream.clone())
    }
    async fn verify_ref(&self, _repo: &Path, _remote_ref: &str) -> anyhow::Result<bool> {
        Ok(self.verify_ref)
    }
    async fn rev_list_count(&self, _repo: &Path, _range: &str) -> anyhow::Result<usize> {
        Ok(self.rev_list_count)
    }
    async fn rev_list_left_right(
        &self,
        _repo: &Path,
        _range: &str,
    ) -> anyhow::Result<(usize, usize)> {
        Ok(self.rev_list_left_right)
    }
    async fn merge_ff_only(&self, _repo: &Path, _remote_ref: &str) -> anyhow::Result<SyncOutput> {
        Ok(self.merge_result.clone())
    }
}

/// Scripted `ManagedManifest`: returns `repos` verbatim, or fails with `error`'s
/// text if set (manifest-parse-failure path).
#[derive(Debug, Default, Clone)]
pub struct FakeManagedManifest {
    pub repos: Vec<ManagedRepo>,
    pub error: Option<String>,
}

impl ManagedManifest for FakeManagedManifest {
    async fn load(&self, _repos_file: &Path, _home_dir: &Path) -> anyhow::Result<Vec<ManagedRepo>> {
        if let Some(message) = &self.error {
            anyhow::bail!("{message}");
        }
        Ok(self.repos.clone())
    }
}

/// Scripted `PushLedger`: `last_known` answers from `known` and writes are no-ops.
#[derive(Debug, Default, Clone)]
pub struct FakePushLedger {
    pub known: HashMap<String, LedgerEntry>,
}

impl PushLedger for FakePushLedger {
    async fn record(&self, _repo_name: &str, _ahead: usize, _checked_at: &str) {}
    async fn last_known(&self, repo_name: &str) -> Option<LedgerEntry> {
        self.known.get(repo_name).cloned()
    }
    async fn refresh(&self) {}
}

/// Provides one clone-shared in-memory `SQLite` connection for application tests.
#[derive(Debug, Clone)]
#[cfg(test)]
pub(crate) struct AppStateStoreTest {
    connection: Arc<Mutex<rusqlite::Connection>>,
}

#[cfg(test)]
impl AppStateStoreTest {
    pub(crate) fn new(schema: &str) -> anyhow::Result<Self> {
        let connection = rusqlite::Connection::open_in_memory()?;
        connection.execute_batch(schema)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
}

#[cfg(test)]
impl AppStateStore for AppStateStoreTest {
    fn connection_lock(
        &self,
    ) -> anyhow::Result<impl std::ops::DerefMut<Target = rusqlite::Connection> + '_> {
        self.connection
            .try_lock()
            .map_err(|error| anyhow::anyhow!("locking app-state test connection failed: {error}"))
    }
}

/// Scripted `RepoProbe`: answers every probe with the configured result.
#[derive(Debug, Clone)]
pub struct FakeRepoProbe {
    pub result: RepoProbeResult,
}

impl Default for FakeRepoProbe {
    fn default() -> Self {
        Self {
            result: RepoProbeResult::NotFound,
        }
    }
}

impl RepoProbe for FakeRepoProbe {
    fn probe(&self, _dir: &Path) -> anyhow::Result<RepoProbeResult> {
        Ok(self.result.clone())
    }
}

/// One recorded [`FakeGitRunner`] invocation: the repo it ran in plus its argv.
pub type RecordedGitCall = (PathBuf, Vec<String>);

/// Scripted `GitRunner`: returns queued outputs or transport errors per call and
/// records every argv it saw (with the repo it ran in).
#[derive(Debug, Clone, Default)]
pub struct FakeGitRunner {
    pub calls: Arc<Mutex<Vec<RecordedGitCall>>>,
    pub results: Arc<Mutex<Vec<GitOutput>>>,
    transport_errors: Arc<Mutex<BTreeMap<usize, anyhow::Error>>>,
    /// Repos `repo_present` answers `false` for; everything else is present.
    pub absent_repos: Arc<Mutex<Vec<PathBuf>>>,
}

impl FakeGitRunner {
    pub fn new(results: Vec<GitOutput>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            results: Arc::new(Mutex::new(results)),
            transport_errors: Arc::new(Mutex::new(BTreeMap::new())),
            absent_repos: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Scripts successful Git outputs and transport errors in invocation order.
    pub fn with_results(results: Vec<anyhow::Result<GitOutput>>) -> Self {
        let mut outputs = Vec::new();
        let mut transport_errors = BTreeMap::new();
        for (index, result) in results.into_iter().enumerate() {
            match result {
                Ok(output) => outputs.push(output),
                Err(error) => {
                    transport_errors.insert(index, error);
                }
            }
        }
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            results: Arc::new(Mutex::new(outputs)),
            transport_errors: Arc::new(Mutex::new(transport_errors)),
            absent_repos: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// A clean-exit output with `stdout`.
    pub fn ok(stdout: &str) -> GitOutput {
        GitOutput {
            stdout: stdout.into(),
            stderr: String::new(),
            exit_code: 0,
        }
    }

    /// A clean-exit output whose message rode on stderr (git does this for pushes).
    pub fn ok_stderr(stderr: &str) -> GitOutput {
        GitOutput {
            stdout: String::new(),
            stderr: stderr.into(),
            exit_code: 0,
        }
    }

    /// A failed output with `stderr` and `code`.
    pub fn exit_err(stderr: &str, code: i32) -> GitOutput {
        GitOutput {
            stdout: String::new(),
            stderr: stderr.into(),
            exit_code: code,
        }
    }

    /// Every recorded argv, without the repo paths.
    ///
    /// # Panics
    ///
    /// Panics when the internal call log's lock is poisoned (a prior test panic).
    pub fn arg_lists(&self) -> Vec<Vec<String>> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(_, args)| args.clone())
            .collect()
    }
}

impl GitRunner for FakeGitRunner {
    fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
        let mut calls = self.calls.lock().unwrap();
        let index = calls.len();
        calls.push((
            repo.to_path_buf(),
            args.iter().map(ToString::to_string).collect(),
        ));
        drop(calls);
        if let Some(error) = self.transport_errors.lock().unwrap().remove(&index) {
            return Err(error);
        }
        Ok(self.results.lock().unwrap().remove(0))
    }

    fn repo_present(&self, repo: &Path) -> bool {
        !self.absent_repos.lock().unwrap().iter().any(|p| p == repo)
    }
}

/// Scripted `RepoDiscovery`: returns a fixed repo list regardless of root/flag.
#[derive(Debug, Clone, Default)]
pub struct FakeRepoDiscovery {
    pub repos: Vec<PathBuf>,
}

impl RepoDiscovery for FakeRepoDiscovery {
    fn find_repos(&self, _root: &Path, _include_worktrees: bool) -> anyhow::Result<Vec<PathBuf>> {
        Ok(self.repos.clone())
    }
}
