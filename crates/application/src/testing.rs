//! In-memory fakes for the application ports (template pattern: cheap-clone
//! shared state so tests keep a handle after passing a fake in).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use domain::{
    diffs::{Commit, DiffKind},
    managed::ManagedRepo,
    viewer::RenderHistoryId,
};

use crate::ports::{
    AppStateError, AppStateStore, ArtifactMeta, ArtifactStore, Clock, DiffSource, HistoryRecord,
    HtmlRenderer, LedgerEntry, LiveViewRecord, ManagedManifest, NewRecentRenderRecord,
    PlacedArtifact, PushLedger, RecentRenderRecord, RemoteSync, RepoProbe, RepoProbeResult,
    SyncOutput,
};

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
        // Full-context re-runs carry the -U flag added by `full_context_args`.
        if args
            .iter()
            .any(|a| a.starts_with("--unified") || a.starts_with("-U"))
        {
            return Ok(self.full_diff_output.clone());
        }
        Ok(self
            .per_repo
            .get(&repo.to_string_lossy().into_owned())
            .map_or_else(|| self.diff_output.clone(), |o| o.diff_output.clone()))
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
/// Scripted range-lookup hits keyed by `(kind, base_sha, head_sha)`.
pub type RangeHits = Arc<Mutex<HashMap<(DiffKind, String, String), PathBuf>>>;

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
    ) -> anyhow::Result<Option<PathBuf>> {
        Ok(self
            .range_hits
            .lock()
            .unwrap()
            .get(&(kind, base_sha.to_string(), head_sha.to_string()))
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
    fn build_html(&self, view: &domain::diffs::View) -> String {
        format!("<html><title>{}</title></html>", view.title)
    }
    fn build_tabbed_html(&self, title: &str, views: &[domain::diffs::View]) -> String {
        format!("<html><title>{title}</title>{} views</html>", views.len())
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

/// Recording `AppStateStore`: plain in-memory maps behind `Arc<Mutex<...>>`
/// so tests keep a handle after moving the fake into a handler.
#[derive(Debug, Default, Clone)]
pub struct InMemoryAppStateStore {
    pub live_views: Arc<Mutex<Vec<LiveViewRecord>>>,
    pub remove_live_view_error: Arc<Mutex<Option<String>>>,
    pub settings: Arc<Mutex<HashMap<String, String>>>,
    pub set_setting_error: Arc<Mutex<Option<String>>>,
    pub renders: Arc<Mutex<Vec<RecentRenderRecord>>>,
    pub list_error_id: Arc<Mutex<Option<i64>>>,
}

impl AppStateStore for InMemoryAppStateStore {
    fn save_live_view(&self, _data_root: &Path, record: &LiveViewRecord) -> anyhow::Result<bool> {
        let mut views = self.live_views.lock().unwrap();
        if let Some(existing) = views
            .iter_mut()
            .find(|v| v.source_kind == record.source_kind && v.source_value == record.source_value)
        {
            existing.display_name.clone_from(&record.display_name);
            return Ok(true);
        }
        views.push(record.clone());
        Ok(false)
    }
    fn list_live_views(&self, _data_root: &Path) -> anyhow::Result<Vec<LiveViewRecord>> {
        Ok(self.live_views.lock().unwrap().clone())
    }
    fn remove_live_view(
        &self,
        _data_root: &Path,
        source_kind: &str,
        source_value: &str,
    ) -> anyhow::Result<bool> {
        if let Some(message) = self.remove_live_view_error.lock().unwrap().as_ref() {
            anyhow::bail!(message.clone());
        }
        let mut views = self.live_views.lock().unwrap();
        let before = views.len();
        views.retain(|v| !(v.source_kind == source_kind && v.source_value == source_value));
        Ok(views.len() != before)
    }
    fn get_setting(&self, _data_root: &Path, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.settings.lock().unwrap().get(key).cloned())
    }
    fn set_setting(&self, _data_root: &Path, key: &str, value: &str) -> anyhow::Result<()> {
        if let Some(message) = self.set_setting_error.lock().unwrap().as_ref() {
            anyhow::bail!(message.clone());
        }
        self.settings
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn record_render(
        &self,
        _data_root: &Path,
        record: &NewRecentRenderRecord,
    ) -> anyhow::Result<()> {
        let mut renders = self.renders.lock().unwrap();
        let next_id = renders
            .iter()
            .map(|render| i64::from(render.id))
            .max()
            .unwrap_or(0)
            + 1;
        renders.push(RecentRenderRecord {
            id: RenderHistoryId::try_new(next_id).expect("generated in-memory row id is positive"),
            recipe_json: record.recipe_json.clone(),
            title: record.title.clone(),
            repo_name: record.repo_name.clone(),
            kind: record.kind.clone(),
            range_label: record.range_label.clone(),
            rendered_at: record.rendered_at.clone(),
        });
        Ok(())
    }
    fn list_recent_renders(
        &self,
        _data_root: &Path,
    ) -> Result<Vec<RecentRenderRecord>, AppStateError> {
        if let Some(id) = *self.list_error_id.lock().unwrap() {
            return Err(AppStateError::InvalidRecentRenderId { id });
        }
        let mut renders = self.renders.lock().unwrap().clone();
        renders.reverse();
        Ok(renders)
    }
    fn get_recent_render(
        &self,
        _data_root: &Path,
        id: RenderHistoryId,
    ) -> Result<Option<RecentRenderRecord>, AppStateError> {
        Ok(self
            .renders
            .lock()
            .unwrap()
            .iter()
            .find(|render| render.id == id)
            .cloned())
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
