//! In-memory fakes for the application ports (template pattern: cheap-clone
//! shared state so tests keep a handle after passing a fake in).

#[cfg(test)]
pub(crate) mod diffs;
#[cfg(test)]
pub(crate) mod viewer;

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use gtl_models::{
    diffs::{Commit, DiffKind},
    managed::ManagedRepo,
    viewer::RenderOptions,
};

use crate::ports::{
    AppSettings, ArtifactMeta, ArtifactStore, Clock, GitClient, GitCommitReceipt, GitDiffFormat,
    GitDiffRequest, GitEffect, GitPushReceipt, GitRepositoryState, GitWorkingTree, HistoryRecord,
    HtmlRenderer, LedgerEntry, ManagedManifest, MergedBranch, PlacedArtifact, PushLedger,
    RepoDiscovery, UserSettingsStore,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitResponse {
    Applied(String),
    Rejected(String),
}

impl GitResponse {
    fn success(&self) -> bool {
        matches!(self, Self::Applied(_))
    }

    fn diagnostic(&self) -> &str {
        match self {
            Self::Applied(detail) | Self::Rejected(detail) => detail.trim(),
        }
    }

    fn error_line(&self) -> String {
        self.diagnostic()
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(|| "Git operation rejected".to_string(), str::to_string)
    }

    fn combined(&self) -> String {
        match self {
            Self::Applied(detail) | Self::Rejected(detail) => detail.clone(),
        }
    }

    fn applied_detail(&self) -> Option<&str> {
        match self {
            Self::Applied(detail) => Some(detail),
            Self::Rejected(_) => None,
        }
    }
}

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

/// Scripted `GitClient`: every field is what the corresponding method returns.
#[derive(Debug, Default, Clone)]
pub struct FakeGitClient {
    pub top_level: Option<String>,
    pub branch: String,
    pub upstream: Option<String>,
    pub commits: Vec<Commit>,
    pub diff_output: String,
    pub full_diff_output: String,
    pub known_revs: Vec<String>,
    pub shas: HashMap<String, String>,
    pub committed_at: String,
    pub repository_state: Option<GitRepositoryState>,
    pub repository_probe_error: Option<String>,
    /// Per-repo overrides for `commits`/`diff_output`, keyed by the `top` path
    /// `build_view` is called with -- lets one scripted source produce different
    /// results (e.g. one repo empty, one not) across a single `render_batch` call,
    /// which otherwise can only script one outcome for every repo.
    pub per_repo: HashMap<String, RepoOverride>,
    pub managed: Option<ManagedGitScript>,
}

/// See [`FakeGitClient::per_repo`].
#[derive(Debug, Default, Clone)]
pub struct RepoOverride {
    pub commits: Vec<Commit>,
    pub diff_output: String,
}

impl FakeGitClient {
    /// The scripted primary diff for `repo`: its per-repo override, else the shared one.
    fn scripted_diff(&self, repo: &Path) -> String {
        self.per_repo
            .get(&repo.to_string_lossy().into_owned())
            .map_or_else(|| self.diff_output.clone(), |o| o.diff_output.clone())
    }
}

impl GitClient for FakeGitClient {
    fn repo_present(&self, _repo: &Path) -> bool {
        self.managed.as_ref().is_none_or(|script| script.present)
    }
    fn probe_repository(&self, _dir: &Path) -> anyhow::Result<GitRepositoryState> {
        if let Some(error) = self.repository_probe_error.as_ref() {
            anyhow::bail!(error.clone());
        }
        Ok(self.repository_state.clone().unwrap_or_else(|| {
            self.top_level
                .as_ref()
                .map_or(GitRepositoryState::NotFound, |top_level| {
                    GitRepositoryState::Repository {
                        top_level: top_level.into(),
                    }
                })
        }))
    }
    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<PathBuf>> {
        Ok(self
            .top_level
            .clone()
            .map(PathBuf::from)
            .or_else(|| Some(dir.to_path_buf())))
    }
    fn top_level(&self, _dir: &Path) -> anyhow::Result<String> {
        self.top_level
            .clone()
            .ok_or_else(|| anyhow::anyhow!("not a git repository"))
    }
    fn current_branch(&self, _repo: &Path) -> anyhow::Result<String> {
        Ok(self
            .managed
            .as_ref()
            .map_or_else(|| self.branch.clone(), |script| script.branch.clone()))
    }
    fn upstream(&self, _repo: &Path) -> anyhow::Result<GitEffect<String>> {
        let upstream = self
            .managed
            .as_ref()
            .map_or_else(|| self.upstream.clone(), |script| script.upstream.clone());
        Ok(upstream.map_or_else(
            || GitEffect::Rejected("no upstream".into()),
            GitEffect::Applied,
        ))
    }
    fn branch_remote(&self, _repo: &Path, _branch: &str) -> anyhow::Result<Option<String>> {
        Ok(Some("origin".into()))
    }
    fn remote_url(&self, _repo: &Path, _remote: &str) -> anyhow::Result<Option<String>> {
        Ok(self
            .managed
            .as_ref()
            .and_then(|script| script.has_remote.then(|| "configured".into())))
    }
    fn revision_exists(&self, _repo: &Path, revision: &str) -> anyhow::Result<bool> {
        Ok(self.managed.as_ref().map_or_else(
            || self.known_revs.iter().any(|known| known == revision),
            |script| script.verify_ref,
        ))
    }
    fn commit_count(&self, _repo: &Path, _range: &str) -> anyhow::Result<Option<usize>> {
        Ok(Some(
            self.managed
                .as_ref()
                .map_or(self.commits.len(), |script| script.rev_list_count),
        ))
    }
    fn ahead_behind(&self, _repo: &Path, _range: &str) -> anyhow::Result<Option<(usize, usize)>> {
        Ok(Some(self.managed.as_ref().map_or_else(
            || (0, self.commits.len()),
            |script| script.rev_list_left_right,
        )))
    }
    fn is_ancestor(
        &self,
        _repo: &Path,
        _ancestor: &str,
        _descendant: &str,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }
    fn working_tree(&self, _repo: &Path) -> anyhow::Result<GitEffect<GitWorkingTree>> {
        Ok(GitEffect::Applied(GitWorkingTree::default()))
    }
    fn merged_branches(
        &self,
        _repo: &Path,
        _into: &str,
    ) -> anyhow::Result<GitEffect<Vec<MergedBranch>>> {
        Ok(GitEffect::Applied(Vec::new()))
    }
    fn worktrees(
        &self,
        _repo: &Path,
    ) -> anyhow::Result<GitEffect<Vec<gtl_models::worktrees::Worktree>>> {
        Ok(GitEffect::Applied(Vec::new()))
    }
    fn local_tags(
        &self,
        _repo: &Path,
    ) -> anyhow::Result<GitEffect<BTreeMap<String, gtl_models::tags::Tag>>> {
        Ok(GitEffect::Applied(BTreeMap::new()))
    }
    fn remote_tags(
        &self,
        _repo: &Path,
        _remote: &str,
    ) -> anyhow::Result<GitEffect<BTreeMap<String, String>>> {
        Ok(GitEffect::Applied(BTreeMap::new()))
    }
    fn previous_checkout(&self, _repo: &Path) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn brief_log(&self, _repo: &Path, _range: &str) -> anyhow::Result<GitEffect<Vec<String>>> {
        Ok(GitEffect::Applied(Vec::new()))
    }
    fn diff_stat(
        &self,
        _repo: &Path,
        _before: &str,
        _after: &str,
    ) -> anyhow::Result<GitEffect<String>> {
        Ok(GitEffect::Applied(String::new()))
    }
    fn stage_all(&self, _repo: &Path) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn commit(&self, _repo: &Path, _message: &str) -> anyhow::Result<GitEffect<GitCommitReceipt>> {
        Ok(GitEffect::Applied(GitCommitReceipt {
            detail: "committed".into(),
            identity: None,
        }))
    }
    fn switch(&self, _repo: &Path, _branch: &str) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn switch_previous(&self, _repo: &Path) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn fast_forward(&self, _repo: &Path, _revision: &str) -> anyhow::Result<GitEffect<String>> {
        Ok(self.managed.as_ref().map_or_else(
            || GitEffect::Applied(String::new()),
            |script| script.merge_result.effect(),
        ))
    }
    fn move_branch(
        &self,
        _repo: &Path,
        _branch: &str,
        _revision: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn delete_branch(&self, _repo: &Path, _branch: &str) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn soft_reset(&self, _repo: &Path, _revision: &str) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn push_branch(
        &self,
        _repo: &Path,
        _remote: &str,
        _branch: &str,
        _dry_run: bool,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>> {
        Ok(self.managed.as_ref().map_or_else(
            || {
                GitEffect::Applied(GitPushReceipt {
                    detail: "pushed".into(),
                    up_to_date: false,
                })
            },
            |script| script.push_result.push_effect(),
        ))
    }
    fn fetch(&self, _repo: &Path, _remote: &str) -> anyhow::Result<GitEffect<String>> {
        Ok(self.managed.as_ref().map_or_else(
            || GitEffect::Applied(String::new()),
            |script| script.fetch_result.effect(),
        ))
    }
    fn create_annotated_tag(
        &self,
        _repo: &Path,
        _tag: &str,
        _message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn create_lightweight_tag(
        &self,
        _repo: &Path,
        _tag: &str,
        _revision: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn push_tag_refs(
        &self,
        _repo: &Path,
        _remote: &str,
        _tags: &[String],
    ) -> anyhow::Result<GitEffect<String>> {
        Ok(GitEffect::Applied(String::new()))
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
    fn diff(&self, repo: &Path, request: &GitDiffRequest) -> anyhow::Result<String> {
        // The exclusion pass asks for paths only; mirror git by listing the
        // scripted diff's file paths, one per line.
        if request.format == GitDiffFormat::NamesOnly {
            let paths: Vec<String> = crate::diffs::util::parse_diff(&self.scripted_diff(repo))
                .into_iter()
                .map(|file| file.path)
                .collect();
            return Ok(paths.join("\n"));
        }
        if request.format == GitDiffFormat::FullContext {
            return Ok(self.full_diff_output.clone());
        }
        Ok(self.scripted_diff(repo))
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
    fn merge_base(&self, _repo: &Path, left: &str, right: &str) -> anyhow::Result<String> {
        Ok(format!("merge-base-{left}-{right}"))
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
        let path = store_root.join("artifact.html");
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

/// Scripted process result used to configure managed Git client behavior.
#[derive(Debug, Default, Clone)]
pub struct SyncOutput {
    pub success: bool,
    pub combined: String,
}

impl SyncOutput {
    fn effect(&self) -> GitEffect<String> {
        if self.success {
            GitEffect::Applied(self.combined.clone())
        } else {
            GitEffect::Rejected(self.combined.trim().to_string())
        }
    }

    fn push_effect(&self) -> GitEffect<GitPushReceipt> {
        if self.success {
            GitEffect::Applied(GitPushReceipt {
                detail: crate::shared::git::last_non_empty_line(&self.combined)
                    .unwrap_or("up to date")
                    .to_string(),
                up_to_date: self.combined.contains("Everything up-to-date"),
            })
        } else {
            GitEffect::Rejected(self.combined.trim().to_string())
        }
    }
}

/// Managed Git behavior layered onto [`FakeGitClient`].
/// Single-scripted (no per-repo map) — every existing push/pull test scripts one
/// repo's worth of git responses, matching `push_pull.rs`'s existing test style.
#[derive(Debug, Default, Clone)]
pub struct ManagedGitScript {
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

impl ManagedGitScript {
    pub fn git_client(self) -> FakeGitClient {
        FakeGitClient {
            managed: Some(self),
            ..FakeGitClient::default()
        }
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

/// Scripted Git client that returns queued adapter responses or failures.
#[derive(Debug, Clone, Default)]
pub struct ScriptedGitClient {
    pub results: Arc<Mutex<Vec<GitResponse>>>,
    transport_errors: Arc<Mutex<BTreeMap<usize, anyhow::Error>>>,
    invocations: Arc<AtomicUsize>,
    /// Repos `repo_present` answers `false` for; everything else is present.
    pub absent_repos: Arc<Mutex<Vec<PathBuf>>>,
}

impl ScriptedGitClient {
    pub fn new(results: Vec<GitResponse>) -> Self {
        Self {
            results: Arc::new(Mutex::new(results)),
            transport_errors: Arc::new(Mutex::new(BTreeMap::new())),
            invocations: Arc::new(AtomicUsize::new(0)),
            absent_repos: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Scripts successful Git outputs and transport errors in invocation order.
    pub fn with_results(results: Vec<anyhow::Result<GitResponse>>) -> Self {
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
            results: Arc::new(Mutex::new(outputs)),
            transport_errors: Arc::new(Mutex::new(transport_errors)),
            invocations: Arc::new(AtomicUsize::new(0)),
            absent_repos: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// An effect accepted by the Git adapter, with optional semantic detail.
    pub fn applied(detail: &str) -> GitResponse {
        GitResponse::Applied(detail.into())
    }

    /// An effect rejected by Git, with its semantic diagnostic.
    pub fn rejected(detail: &str) -> GitResponse {
        GitResponse::Rejected(detail.into())
    }
}

impl ScriptedGitClient {
    fn respond(&self) -> anyhow::Result<GitResponse> {
        let index = self.invocations.fetch_add(1, Ordering::Relaxed);
        if let Some(error) = self.transport_errors.lock().unwrap().remove(&index) {
            return Err(error);
        }
        Ok(self.results.lock().unwrap().remove(0))
    }

    fn repo_present(&self, repo: &Path) -> bool {
        !self.absent_repos.lock().unwrap().iter().any(|p| p == repo)
    }
}

impl GitClient for ScriptedGitClient {
    fn repo_present(&self, repo: &Path) -> bool {
        self.repo_present(repo)
    }
    fn probe_repository(&self, dir: &Path) -> anyhow::Result<GitRepositoryState> {
        if !self.repo_present(dir) {
            return Ok(GitRepositoryState::NotFound);
        }
        Ok(scripted_capture(self, dir, &[])?.map_or(
            GitRepositoryState::NotARepository,
            |top_level| GitRepositoryState::Repository {
                top_level: top_level.into(),
            },
        ))
    }
    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<PathBuf>> {
        scripted_capture(self, dir, &["rev-parse", "--show-toplevel"])
            .map(|top| top.map(PathBuf::from))
    }
    fn top_level(&self, dir: &Path) -> anyhow::Result<String> {
        scripted_capture(self, dir, &["rev-parse", "--show-toplevel"])?
            .ok_or_else(|| anyhow::anyhow!("not a git repository"))
    }
    fn current_branch(&self, repo: &Path) -> anyhow::Result<String> {
        scripted_capture(self, repo, &["rev-parse", "--abbrev-ref", "HEAD"])?
            .ok_or_else(|| anyhow::anyhow!("not a git repository"))
    }
    fn upstream(&self, repo: &Path) -> anyhow::Result<GitEffect<String>> {
        scripted_effect(
            self,
            repo,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            |output| output.trim().to_string(),
        )
    }
    fn branch_remote(&self, repo: &Path, branch: &str) -> anyhow::Result<Option<String>> {
        scripted_capture(self, repo, &["config", &format!("branch.{branch}.remote")])
    }
    fn remote_url(&self, repo: &Path, remote: &str) -> anyhow::Result<Option<String>> {
        scripted_capture(self, repo, &["remote", "get-url", remote])
    }
    fn revision_exists(&self, repo: &Path, revision: &str) -> anyhow::Result<bool> {
        scripted_success(self, repo, &["rev-parse", "--verify", revision])
    }
    fn commit_count(&self, repo: &Path, range: &str) -> anyhow::Result<Option<usize>> {
        Ok(
            scripted_capture(self, repo, &["rev-list", "--count", range])?
                .and_then(|count| count.parse().ok()),
        )
    }
    fn ahead_behind(&self, repo: &Path, range: &str) -> anyhow::Result<Option<(usize, usize)>> {
        Ok(
            scripted_capture(self, repo, &["rev-list", "--count", "--left-right", range])?
                .and_then(|counts| {
                    let (left, right) = counts.split_once(char::is_whitespace)?;
                    Some((left.parse().ok()?, right.trim().parse().ok()?))
                }),
        )
    }
    fn is_ancestor(&self, repo: &Path, ancestor: &str, descendant: &str) -> anyhow::Result<bool> {
        scripted_success(
            self,
            repo,
            &["merge-base", "--is-ancestor", ancestor, descendant],
        )
    }
    fn working_tree(&self, repo: &Path) -> anyhow::Result<GitEffect<GitWorkingTree>> {
        scripted_effect(self, repo, &["status", "--porcelain"], |output| {
            let mut tree = GitWorkingTree::default();
            for line in output.lines().filter(|line| !line.is_empty()) {
                let bytes = line.as_bytes();
                if bytes.len() < 2 {
                    continue;
                }
                tree.files
                    .push(gtl_models::managed::working_tree::CommitFile {
                        status: line.get(0..2).unwrap_or("").trim().to_string(),
                        path: line.get(3..).unwrap_or("").to_string(),
                    });
                let (index, worktree) = (bytes[0], bytes[1]);
                if index == b'?' && worktree == b'?' {
                    tree.unprepared += 1;
                } else {
                    tree.staged += usize::from(index != b' ');
                    tree.unprepared += usize::from(worktree != b' ');
                }
            }
            tree
        })
    }
    fn merged_branches(
        &self,
        repo: &Path,
        into: &str,
    ) -> anyhow::Result<GitEffect<Vec<MergedBranch>>> {
        scripted_effect(
            self,
            repo,
            &[
                "for-each-ref",
                "--merged",
                into,
                "--format=%(refname:short) %(objectname:short)",
                "refs/heads/",
            ],
            |output| {
                output
                    .lines()
                    .filter_map(|line| {
                        let (name, sha) = line.trim().split_once(char::is_whitespace)?;
                        Some(MergedBranch {
                            name: name.to_string(),
                            sha: sha.trim().to_string(),
                        })
                    })
                    .collect()
            },
        )
    }
    fn worktrees(
        &self,
        repo: &Path,
    ) -> anyhow::Result<GitEffect<Vec<gtl_models::worktrees::Worktree>>> {
        scripted_effect(self, repo, &["worktree", "list", "--porcelain"], |output| {
            crate::worktrees::porcelain::parse(output)
        })
    }
    fn local_tags(
        &self,
        repo: &Path,
    ) -> anyhow::Result<GitEffect<BTreeMap<String, gtl_models::tags::Tag>>> {
        scripted_effect(
            self,
            repo,
            &[
                "for-each-ref",
                crate::tags::parse::LOCAL_TAG_FORMAT_ARG,
                "refs/tags",
            ],
            crate::tags::parse::parse_refs,
        )
    }
    fn remote_tags(
        &self,
        repo: &Path,
        remote: &str,
    ) -> anyhow::Result<GitEffect<BTreeMap<String, String>>> {
        scripted_effect(self, repo, &["ls-remote", "--tags", remote], |output| {
            crate::tags::parse::parse_remote_refs(output)
        })
    }
    fn previous_checkout(&self, repo: &Path) -> anyhow::Result<Option<String>> {
        scripted_capture(self, repo, &["rev-parse", "@{-1}"])
    }
    fn brief_log(&self, repo: &Path, range: &str) -> anyhow::Result<GitEffect<Vec<String>>> {
        scripted_effect(self, repo, &["log", "--format=%h %s", range], |output| {
            output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
    }
    fn diff_stat(
        &self,
        repo: &Path,
        before: &str,
        after: &str,
    ) -> anyhow::Result<GitEffect<String>> {
        scripted_effect(
            self,
            repo,
            &["diff", "--stat", before, after],
            str::to_string,
        )
    }
    fn stage_all(&self, repo: &Path) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo, &["add", "-A"], |_| ())
    }
    fn commit(&self, repo: &Path, message: &str) -> anyhow::Result<GitEffect<GitCommitReceipt>> {
        scripted_effect(self, repo, &["commit", "-m", message], |output| {
            GitCommitReceipt {
                detail: crate::shared::git::last_non_empty_line(output)
                    .unwrap_or("committed")
                    .to_string(),
                identity: commit_identity(output),
            }
        })
    }
    fn switch(&self, repo: &Path, branch: &str) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo, &["switch", branch], |_| ())
    }
    fn switch_previous(&self, repo: &Path) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo, &["switch", "-"], |_| ())
    }
    fn fast_forward(&self, repo: &Path, revision: &str) -> anyhow::Result<GitEffect<String>> {
        scripted_effect(
            self,
            repo,
            &["merge", "--ff-only", revision],
            str::to_string,
        )
    }
    fn move_branch(
        &self,
        repo: &Path,
        branch: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo, &["branch", "-f", branch, revision], |_| ())
    }
    fn delete_branch(&self, repo: &Path, branch: &str) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo, &["branch", "-D", branch], |_| ())
    }
    fn soft_reset(&self, repo: &Path, revision: &str) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo, &["reset", "--soft", revision], |_| ())
    }
    fn push_branch(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        dry_run: bool,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>> {
        let mut args = vec!["push", remote, branch];
        if dry_run {
            args.push("--dry-run");
        }
        scripted_effect(self, repo, &args, |output| GitPushReceipt {
            detail: crate::shared::git::last_non_empty_line(output)
                .unwrap_or("pushed")
                .to_string(),
            up_to_date: output.contains("Everything up-to-date"),
        })
    }
    fn fetch(&self, repo: &Path, remote: &str) -> anyhow::Result<GitEffect<String>> {
        scripted_effect(self, repo, &["fetch", remote], str::to_string)
    }
    fn create_annotated_tag(
        &self,
        repo: &Path,
        tag: &str,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo, &["tag", "-a", tag, "-m", message], |_| ())
    }
    fn create_lightweight_tag(
        &self,
        repo: &Path,
        tag: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(
            self,
            repo,
            &["tag", tag, &format!("{revision}^{{}}")],
            |_| (),
        )
    }
    fn push_tag_refs(
        &self,
        repo: &Path,
        remote: &str,
        tags: &[String],
    ) -> anyhow::Result<GitEffect<String>> {
        let mut owned = vec!["push".to_string(), remote.to_string()];
        owned.extend(
            tags.iter()
                .map(|tag| format!("refs/tags/{tag}:refs/tags/{tag}")),
        );
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        scripted_effect(self, repo, &args, str::to_string)
    }
    fn verify_commit(&self, repo: &Path, revision: &str) -> anyhow::Result<()> {
        scripted_success(
            self,
            repo,
            &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
        )?
        .then_some(())
        .ok_or_else(|| anyhow::anyhow!("not a commit"))
    }
    fn short_ref(&self, repo: &Path, revision: &str) -> anyhow::Result<String> {
        scripted_capture(self, repo, &["rev-parse", "--short", revision])?
            .ok_or_else(|| anyhow::anyhow!("unknown revision"))
    }
    fn log_commits(&self, repo: &Path, range: &str) -> anyhow::Result<Vec<Commit>> {
        let Some(raw) = scripted_capture(
            self,
            repo,
            &[
                "log",
                "--date=format:%Y-%m-%d %H:%M",
                "--format=%H%x1f%s%x1f%b%x1f%ad%x1f%aI%x1f%P%x1e",
                range,
            ],
        )?
        else {
            return Ok(Vec::new());
        };
        Ok(raw
            .split('\u{1e}')
            .filter_map(|record| {
                let mut fields = record.trim().split('\u{1f}');
                let sha = fields.next()?.trim();
                (!sha.is_empty()).then(|| Commit {
                    sha: sha.chars().take(9).collect(),
                    subject: fields.next().unwrap_or_default().trim().to_string(),
                    body: fields.next().unwrap_or_default().trim().to_string(),
                    date: fields.next().unwrap_or_default().trim().to_string(),
                    iso: fields.next().unwrap_or_default().trim().to_string(),
                    parents: fields
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .map(|parent| parent.chars().take(9).collect())
                        .collect(),
                })
            })
            .collect())
    }
    fn diff(&self, _repo: &Path, _request: &GitDiffRequest) -> anyhow::Result<String> {
        unreachable!("diff tests use FakeGitClient")
    }
    fn root_commit(&self, _repo: &Path) -> Option<String> {
        None
    }
    fn resolve_sha(&self, repo: &Path, revision: &str) -> anyhow::Result<String> {
        scripted_capture(self, repo, &["rev-parse", revision])?
            .ok_or_else(|| anyhow::anyhow!("unknown revision"))
    }
    fn merge_base(&self, repo: &Path, left: &str, right: &str) -> anyhow::Result<String> {
        scripted_capture(self, repo, &["merge-base", left, right])?
            .ok_or_else(|| anyhow::anyhow!("no merge base"))
    }
    fn committed_at(&self, _repo: &Path, _revision: &str) -> String {
        String::new()
    }
}

fn commit_identity(output: &str) -> Option<String> {
    output.lines().rev().find_map(|line| {
        let line = strip_ansi_csi(line);
        let (header, subject) = line.trim().strip_prefix('[')?.split_once(']')?;
        if subject.trim().is_empty() {
            return None;
        }
        let identity = header.split_whitespace().next_back()?;
        ((4..=64).contains(&identity.len())
            && identity.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| identity.to_string())
    })
}

fn strip_ansi_csi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' || chars.next_if_eq(&'[').is_none() {
            output.push(character);
            continue;
        }
        for control_character in chars.by_ref() {
            if ('@'..='~').contains(&control_character) {
                break;
            }
        }
    }
    output
}

fn scripted_capture(
    git: &ScriptedGitClient,
    _repo: &Path,
    _args: &[&str],
) -> anyhow::Result<Option<String>> {
    let output = git.respond()?;
    Ok(output
        .success()
        .then(|| {
            output
                .applied_detail()
                .unwrap_or_default()
                .trim()
                .to_string()
        })
        .filter(|output| !output.is_empty()))
}

fn scripted_success(git: &ScriptedGitClient, _repo: &Path, _args: &[&str]) -> anyhow::Result<bool> {
    git.respond().map(|output| output.success())
}

fn scripted_effect<T>(
    git: &ScriptedGitClient,
    _repo: &Path,
    _args: &[&str],
    applied: impl FnOnce(&str) -> T,
) -> anyhow::Result<GitEffect<T>> {
    let output = git.respond()?;
    if output.success() {
        Ok(GitEffect::Applied(applied(&output.combined())))
    } else {
        Ok(GitEffect::Rejected(output.error_line()))
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
