mod tags;

#[cfg(test)]
pub(crate) mod diffs;
#[cfg(test)]
pub(crate) mod viewer;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
};

#[cfg(test)]
use gtl_models::diffs::PinnedRange;
#[cfg(test)]
use gtl_models::paths::ProjectName;
use gtl_models::{
    diffs::{Commit, CommitId, CommitIdError, DiffExclusions},
    git::{
        AheadBehind, BranchName, CommitCount, GitEffectMode, GitHead, GitObjectId, GitRange,
        GitRefName, GitRevision, RemoteName, RemoteUrl, TagName,
    },
    paths::{AbsoluteFilePath, RepositoryRelativePath, RepositoryRoot},
    projects::ProjectRepository,
    settings::{PushAllExclusions, UserSettings},
    timestamps::MachineTimestamp,
    viewer::{RenderOptions, Theme},
};

#[cfg(test)]
#[must_use]
pub(crate) fn repository_root(path: &str) -> RepositoryRoot {
    RepositoryRoot::try_new(path.into()).unwrap()
}

#[cfg(test)]
#[must_use]
pub(crate) fn project_name(name: &str) -> ProjectName {
    ProjectName::try_new(name.to_owned()).unwrap()
}

#[cfg(test)]
#[must_use]
pub(crate) fn repository_relative_path(path: &str) -> RepositoryRelativePath {
    RepositoryRelativePath::try_new(path.into()).unwrap()
}

#[cfg(test)]
#[must_use]
pub(crate) fn absolute_file_path(path: &str) -> AbsoluteFilePath {
    AbsoluteFilePath::try_new(path.into()).unwrap()
}

use crate::ports::{
    ArtifactMeta, ArtifactRangeKey, ArtifactStore, Clock, GitClient, GitCommitReceipt,
    GitDiffFormat, GitDiffRequest, GitEffect, GitPushReceipt, GitRepositoryState, GitWorkingTree,
    HistoryRecord, HtmlRenderer, PlacedArtifact, ProjectCatalogueUnavailableError, ProjectClient,
    ProjectClientError, UserSettingsLoadError, UserSettingsReader,
};

fn try_commit_id_fixture(raw: &str) -> Result<CommitId, CommitIdError> {
    if let Ok(id) = raw.try_into() {
        return Ok(id);
    }
    let seed = if !raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        raw.to_ascii_lowercase()
    } else {
        let encoded = raw
            .bytes()
            .flat_map(|byte| {
                [
                    "0123456789abcdef".as_bytes()[usize::from(byte >> 4)],
                    "0123456789abcdef".as_bytes()[usize::from(byte & 0x0f)],
                ]
            })
            .map(char::from)
            .collect::<String>();
        if encoded.is_empty() {
            "0".to_owned()
        } else {
            encoded
        }
    };
    let value = seed.chars().cycle().take(40).collect::<String>();
    value.try_into()
}

#[cfg(test)]
pub(crate) fn commit_id_fixture(raw: &str) -> CommitId {
    try_commit_id_fixture(raw).unwrap()
}

#[cfg(test)]
pub(crate) fn pinned_range(base: &str, head: &str) -> PinnedRange {
    PinnedRange {
        base: commit_id_fixture(base),
        head: commit_id_fixture(head),
    }
}

#[cfg(test)]
pub(crate) fn git_revision(raw: &str) -> GitRevision {
    GitRevision::try_new(raw.to_owned()).unwrap()
}

#[cfg(test)]
pub(crate) fn git_range(raw: &str) -> GitRange {
    GitRange::try_new(raw.to_owned()).unwrap()
}

#[cfg(test)]
pub(crate) fn branch_name(raw: &str) -> BranchName {
    BranchName::try_new(raw.to_owned()).unwrap()
}

#[cfg(test)]
pub(crate) fn remote_name(raw: &str) -> RemoteName {
    RemoteName::try_new(raw.to_owned()).unwrap()
}

#[cfg(test)]
pub(crate) fn tag_name(raw: &str) -> TagName {
    TagName::try_new(raw.to_owned()).unwrap()
}

#[cfg(test)]
pub(crate) fn git_object_id(raw: &str) -> GitObjectId {
    GitObjectId::from(&commit_id_fixture(raw))
}

fn git_head_fixture(raw: &str) -> anyhow::Result<GitHead> {
    if raw.is_empty() || raw == "HEAD" {
        Ok(GitHead::Detached)
    } else {
        Ok(GitHead::Branch(BranchName::try_new(raw.to_owned())?))
    }
}

#[cfg(test)]
pub(crate) fn git_head(raw: &str) -> GitHead {
    git_head_fixture(raw).unwrap()
}

#[cfg(test)]
pub(crate) fn commit(id: &str, subject: impl Into<String>) -> Commit {
    Commit {
        id: commit_id_fixture(id),
        subject: subject.into(),
        body: String::new(),
        committed_at: MachineTimestamp::try_from("2026-01-01T00:00:00Z").unwrap(),
        parents: Vec::new(),
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

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

#[derive(Debug, Clone)]
pub struct FixedUserSettingsStore {
    settings: UserSettings,
}

impl FixedUserSettingsStore {
    #[must_use]
    pub const fn new(settings: UserSettings) -> Self {
        Self { settings }
    }
}

#[must_use]
pub fn default_user_settings() -> UserSettings {
    UserSettings::new(
        None,
        RenderOptions::DEFAULT,
        gtl_models::viewer::ViewerKeybindings::default(),
        true,
        DiffExclusions::default(),
        PushAllExclusions::default(),
    )
}

impl Default for FixedUserSettingsStore {
    fn default() -> Self {
        Self::new(default_user_settings())
    }
}

impl UserSettingsReader for FixedUserSettingsStore {
    fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
        Ok(self.settings.clone())
    }
}

#[derive(Debug, Clone)]
pub struct SequenceUserSettingsStore {
    snapshots: Arc<Mutex<VecDeque<UserSettings>>>,
}

impl SequenceUserSettingsStore {
    pub fn new(snapshots: impl IntoIterator<Item = UserSettings>) -> Self {
        Self {
            snapshots: Arc::new(Mutex::new(snapshots.into_iter().collect())),
        }
    }
}

impl UserSettingsReader for SequenceUserSettingsStore {
    fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
        lock_or_recover(&self.snapshots)
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("no configured test settings snapshot remains").into())
    }
}

#[derive(Debug, Default, Clone)]
pub struct FakeGitClient {
    pub top_level: Option<String>,
    pub branch: String,
    pub upstream: Option<String>,
    pub commits: Vec<Commit>,
    pub diff_output: String,
    pub full_diff_output: String,
    pub known_revs: Vec<String>,
    pub commit_ids: HashMap<String, CommitId>,
    pub committed_at: Option<MachineTimestamp>,
    pub repository_state: Option<GitRepositoryState>,
    pub repository_probe_error: Option<String>,
    pub per_repo: HashMap<String, RepoOverride>,
    pub project: Option<ProjectGitScript>,
}

#[derive(Debug, Default, Clone)]
pub struct RepoOverride {
    pub commits: Vec<Commit>,
    pub diff_output: String,
}

impl FakeGitClient {
    fn scripted_diff(&self, repo_path: &RepositoryRoot) -> String {
        self.per_repo
            .get(&repo_path.to_string_lossy().into_owned())
            .map_or_else(|| self.diff_output.clone(), |o| o.diff_output.clone())
    }
}

fn diff_paths(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| {
            line.strip_prefix("diff --git a/")
                .and_then(|rest| rest.split_once(" b/"))
                .map(|(_, path)| path.to_string())
        })
        .collect()
}

impl GitClient for FakeGitClient {
    fn repo_present(&self, _repo: &RepositoryRoot) -> bool {
        self.project.as_ref().is_none_or(|script| script.present)
    }
    fn probe_repository(&self, _dir: &Path) -> anyhow::Result<GitRepositoryState> {
        if let Some(error) = self.repository_probe_error.as_ref() {
            anyhow::bail!(error.clone());
        }
        if let Some(state) = &self.repository_state {
            return Ok(state.clone());
        }
        let Some(top_level) = &self.top_level else {
            return Ok(GitRepositoryState::NotFound);
        };
        Ok(GitRepositoryState::Repository {
            top_level: RepositoryRoot::try_new(top_level.into())?,
        })
    }
    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<RepositoryRoot>> {
        self.top_level
            .clone()
            .map(PathBuf::from)
            .or_else(|| Some(dir.to_path_buf()))
            .map(RepositoryRoot::try_new)
            .transpose()
            .map_err(Into::into)
    }
    fn top_level(&self, _dir: &Path) -> anyhow::Result<RepositoryRoot> {
        self.top_level
            .clone()
            .map(PathBuf::from)
            .map(RepositoryRoot::try_new)
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("not a git repository"))
    }
    fn current_branch(&self, _repo: &RepositoryRoot) -> anyhow::Result<GitHead> {
        let branch = self
            .project
            .as_ref()
            .map_or_else(|| self.branch.clone(), |script| script.branch.clone());
        git_head_fixture(&branch)
    }
    fn upstream(&self, _repo: &RepositoryRoot) -> anyhow::Result<GitEffect<GitRefName>> {
        let upstream = self
            .project
            .as_ref()
            .map_or_else(|| self.upstream.clone(), |script| script.upstream.clone());
        match upstream {
            Some(upstream) => Ok(GitEffect::Applied(GitRefName::try_new(upstream)?)),
            None => Ok(GitEffect::Rejected("no upstream".into())),
        }
    }
    fn branch_remote(
        &self,
        _repo: &RepositoryRoot,
        _branch: &BranchName,
    ) -> anyhow::Result<Option<RemoteName>> {
        Ok(Some(RemoteName::origin()))
    }
    fn remote_url(
        &self,
        _repo: &RepositoryRoot,
        _remote: &RemoteName,
    ) -> anyhow::Result<Option<RemoteUrl>> {
        self.project
            .as_ref()
            .filter(|script| script.has_remote)
            .map(|_| RemoteUrl::try_new("configured"))
            .transpose()
            .map_err(Into::into)
    }
    fn remote_push_urls(
        &self,
        _repo: &RepositoryRoot,
        _remote: &RemoteName,
    ) -> anyhow::Result<Vec<RemoteUrl>> {
        self.project
            .as_ref()
            .filter(|script| script.has_remote)
            .map(|_| RemoteUrl::try_new("configured"))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
    fn revision_exists(
        &self,
        _repo: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<bool> {
        Ok(self.project.as_ref().map_or_else(
            || {
                self.known_revs
                    .iter()
                    .any(|known| known == revision.as_ref())
            },
            |script| script.verify_ref,
        ))
    }
    fn commit_count(
        &self,
        _repo: &RepositoryRoot,
        _range: &GitRange,
    ) -> anyhow::Result<Option<CommitCount>> {
        let count = self
            .project
            .as_ref()
            .map_or(self.commits.len(), |script| script.rev_list_count);
        Ok(Some(CommitCount::new(u64::try_from(count)?)))
    }
    fn ahead_behind(
        &self,
        _repo: &RepositoryRoot,
        _range: &GitRange,
    ) -> anyhow::Result<Option<AheadBehind>> {
        let (behind, ahead) = self.project.as_ref().map_or_else(
            || (0, self.commits.len()),
            |script| script.rev_list_left_right,
        );
        Ok(Some(AheadBehind {
            behind: CommitCount::new(u64::try_from(behind)?),
            ahead: CommitCount::new(u64::try_from(ahead)?),
        }))
    }
    fn working_tree(&self, _repo: &RepositoryRoot) -> anyhow::Result<GitEffect<GitWorkingTree>> {
        Ok(GitEffect::Applied(GitWorkingTree::default()))
    }
    fn local_tags(
        &self,
        _repo: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, gtl_models::tags::Tag>>> {
        Ok(GitEffect::Applied(BTreeMap::new()))
    }
    fn remote_tags(
        &self,
        _repo: &RepositoryRoot,
        _remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, GitObjectId>>> {
        Ok(GitEffect::Applied(BTreeMap::new()))
    }
    fn stage_all(&self, _repo: &RepositoryRoot) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn commit(
        &self,
        _repo: &RepositoryRoot,
        _message: &str,
    ) -> anyhow::Result<GitEffect<GitCommitReceipt>> {
        Ok(GitEffect::Applied(GitCommitReceipt {
            detail: "committed".into(),
            id: try_commit_id_fixture("c")?,
        }))
    }
    fn fast_forward(
        &self,
        _repo: &RepositoryRoot,
        _revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<String>> {
        Ok(self.project.as_ref().map_or_else(
            || GitEffect::Applied(String::new()),
            |script| script.merge_result.effect(),
        ))
    }
    fn push_branch(
        &self,
        _repo: &RepositoryRoot,
        _remote: &RemoteName,
        _branch: &BranchName,
        _mode: GitEffectMode,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>> {
        Ok(self.project.as_ref().map_or_else(
            || {
                GitEffect::Applied(GitPushReceipt::Updated {
                    detail: "pushed".into(),
                })
            },
            |script| script.push_result.push_effect(),
        ))
    }
    fn fetch(
        &self,
        _repo: &RepositoryRoot,
        _remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<String>> {
        Ok(self.project.as_ref().map_or_else(
            || GitEffect::Applied(String::new()),
            |script| script.fetch_result.effect(),
        ))
    }
    fn create_annotated_tag(
        &self,
        _repo: &RepositoryRoot,
        _tag: &TagName,
        _message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn create_annotated_tag_at(
        &self,
        _repo: &RepositoryRoot,
        _tag: &TagName,
        _revision: &GitRevision,
        _message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn create_lightweight_tag(
        &self,
        _repo: &RepositoryRoot,
        _tag: &TagName,
        _revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<()>> {
        Ok(GitEffect::Applied(()))
    }
    fn push_tag_refs(
        &self,
        _repo: &RepositoryRoot,
        _remote: &RemoteName,
        _tags: &BTreeSet<GitRefName>,
    ) -> anyhow::Result<GitEffect<String>> {
        Ok(GitEffect::Applied(String::new()))
    }
    fn verify_commit(&self, _repo: &RepositoryRoot, rev: &GitRevision) -> anyhow::Result<()> {
        if self.known_revs.iter().any(|r| r == rev.as_ref()) {
            Ok(())
        } else {
            anyhow::bail!("unknown revision {rev}")
        }
    }
    fn log_commits(
        &self,
        repo_path: &RepositoryRoot,
        _range: &GitRange,
    ) -> anyhow::Result<Vec<Commit>> {
        Ok(self
            .per_repo
            .get(&repo_path.to_string_lossy().into_owned())
            .map_or_else(|| self.commits.clone(), |o| o.commits.clone()))
    }
    fn diff(&self, repo_path: &RepositoryRoot, request: &GitDiffRequest) -> anyhow::Result<String> {
        if request.format == GitDiffFormat::NamesOnly {
            return Ok(diff_paths(&self.scripted_diff(repo_path)).join("\n"));
        }
        if request.format == GitDiffFormat::FullContext {
            return Ok(self.full_diff_output.clone());
        }
        Ok(self.scripted_diff(repo_path))
    }
    fn resolve_commit_id(
        &self,
        _repo: &RepositoryRoot,
        rev: &GitRevision,
    ) -> anyhow::Result<CommitId> {
        self.commit_ids.get(rev.as_ref()).cloned().map_or_else(
            || try_commit_id_fixture(rev.as_ref()).map_err(Into::into),
            Ok,
        )
    }
    fn merge_base(
        &self,
        _repo: &RepositoryRoot,
        left: &GitRevision,
        right: &GitRevision,
    ) -> anyhow::Result<CommitId> {
        try_commit_id_fixture(&format!("merge-base-{left}-{right}")).map_err(Into::into)
    }
    fn committed_at(&self, _repo: &RepositoryRoot, _rev: &GitRevision) -> Option<MachineTimestamp> {
        self.committed_at.clone()
    }
}

pub type RangeHits = Arc<Mutex<HashMap<ArtifactRangeKey, AbsoluteFilePath>>>;

#[derive(Debug, Clone, PartialEq)]
pub struct StoredArtifact {
    pub meta: ArtifactMeta,
    pub html: String,
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryArtifactStore {
    pub artifacts: Arc<Mutex<HashMap<PathBuf, StoredArtifact>>>,
    pub range_hits: RangeHits,
    pub history: Vec<HistoryRecord>,
}

impl InMemoryArtifactStore {
    #[must_use]
    pub fn artifact(&self, path: &Path) -> Option<StoredArtifact> {
        self.artifacts.lock().ok()?.get(path).cloned()
    }

    #[cfg(test)]
    pub(crate) fn range_hit_insert(&self, key: ArtifactRangeKey, artifact_path: &str) {
        lock_or_recover(&self.range_hits).insert(key, absolute_file_path(artifact_path));
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
        lock_or_recover(&self.artifacts).insert(
            path.clone(),
            StoredArtifact {
                meta: meta.clone(),
                html: html.to_string(),
            },
        );
        Ok(PlacedArtifact::Created {
            path: AbsoluteFilePath::try_new(path)?,
        })
    }
    fn lookup_by_range(
        &self,
        _store_root: &Path,
        _repo_root: &RepositoryRoot,
        key: &ArtifactRangeKey,
    ) -> anyhow::Result<Option<AbsoluteFilePath>> {
        Ok(lock_or_recover(&self.range_hits).get(key).cloned())
    }
    fn list_history(&self, _store_root: &Path) -> anyhow::Result<Vec<HistoryRecord>> {
        Ok(self.history.clone())
    }
}

#[derive(Debug, Default, Clone)]
pub struct StubRenderer;

impl HtmlRenderer for StubRenderer {
    fn build_html(
        &self,
        view: &crate::diffs::View,
        options: RenderOptions,
        theme: Option<Theme>,
        language: gtl_models::settings::ViewerLanguage,
    ) -> anyhow::Result<String> {
        let theme = theme.map_or_else(String::new, |theme| theme.to_string());
        Ok(format!(
            "<html lang=\"{}\" data-theme=\"{}\" data-layout=\"{}\" data-density=\"{}\"><title>{}</title></html>",
            language,
            theme,
            options.layout(),
            options.density(),
            view.title
        ))
    }
    fn build_tabbed_html(
        &self,
        title: &str,
        views: &[crate::diffs::View],
        options: RenderOptions,
        theme: Option<Theme>,
        language: gtl_models::settings::ViewerLanguage,
    ) -> anyhow::Result<String> {
        let theme = theme.map_or_else(String::new, |theme| theme.to_string());
        let view_summaries = views
            .iter()
            .map(|view| {
                format!(
                    "{}:{}:{}:{}:{}:{}",
                    view.repo_name,
                    language,
                    theme,
                    options.layout(),
                    options.density(),
                    view.files.len()
                )
            })
            .collect::<Vec<_>>()
            .join("|");
        Ok(format!(
            "<html><title>{title}</title>{view_summaries}</html>"
        ))
    }
}

#[derive(Debug, Clone)]
pub struct FixedClock(MachineTimestamp);

impl FixedClock {
    #[must_use]
    pub const fn new(timestamp: MachineTimestamp) -> Self {
        Self(timestamp)
    }

    #[cfg(test)]
    pub(crate) fn from_raw(raw: &str) -> Self {
        Self(raw.try_into().unwrap())
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Result<MachineTimestamp, gtl_models::timestamps::TimestampError> {
        Ok(self.0.clone())
    }
}

#[derive(Debug, Default, Clone)]
pub struct SyncOutput {
    pub success: bool,
    pub combined: String,
}

fn last_non_empty_line(output: &str) -> Option<&str> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
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
        if !self.success {
            return GitEffect::Rejected(self.combined.trim().to_string());
        }
        GitEffect::Applied(push_receipt(&self.combined, "up to date"))
    }
}

fn push_receipt(output: &str, detail_fallback: &str) -> GitPushReceipt {
    let detail = last_non_empty_line(output)
        .unwrap_or(detail_fallback)
        .to_string();
    if output.contains("Everything up-to-date") {
        GitPushReceipt::UpToDate { detail }
    } else {
        GitPushReceipt::Updated { detail }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ProjectGitScript {
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

impl ProjectGitScript {
    #[must_use]
    pub fn git_client(self) -> FakeGitClient {
        FakeGitClient {
            project: Some(self),
            ..FakeGitClient::default()
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct FakeProjectClient {
    pub repos: Vec<ProjectRepository>,
    pub error: Option<String>,
}

impl ProjectClient for FakeProjectClient {
    fn list_projects(
        &self,
    ) -> impl Future<Output = Result<Vec<ProjectRepository>, ProjectClientError>> + Send {
        let result =
            match &self.error {
                Some(message) => Err(ProjectCatalogueUnavailableError::Dependency(
                    anyhow::Error::new(std::io::Error::other(message.clone())),
                )
                .into()),
                None => Ok(self.repos.clone()),
            };
        std::future::ready(result)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedGitClient {
    pub results: Arc<Mutex<VecDeque<GitResponse>>>,
    transport_errors: Arc<Mutex<BTreeMap<usize, anyhow::Error>>>,
    invocations: Arc<AtomicUsize>,
    pub absent_repos: Arc<Mutex<Vec<PathBuf>>>,
}

impl ScriptedGitClient {
    #[must_use]
    pub fn new(results: Vec<GitResponse>) -> Self {
        Self {
            results: Arc::new(Mutex::new(results.into())),
            transport_errors: Arc::new(Mutex::new(BTreeMap::new())),
            invocations: Arc::new(AtomicUsize::new(0)),
            absent_repos: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Scripts outputs and transport failures by invocation order.
    #[must_use]
    pub fn with_results(results: Vec<anyhow::Result<GitResponse>>) -> Self {
        let mut outputs = Vec::new();
        let mut transport_errors = BTreeMap::new();
        for (index, result) in results.into_iter().enumerate() {
            record_script_result(&mut outputs, &mut transport_errors, index, result);
        }
        Self {
            results: Arc::new(Mutex::new(outputs.into())),
            transport_errors: Arc::new(Mutex::new(transport_errors)),
            invocations: Arc::new(AtomicUsize::new(0)),
            absent_repos: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn applied(detail: &str) -> GitResponse {
        GitResponse::Applied(detail.into())
    }

    #[must_use]
    pub fn rejected(detail: &str) -> GitResponse {
        GitResponse::Rejected(detail.into())
    }
}

fn record_script_result(
    outputs: &mut Vec<GitResponse>,
    transport_errors: &mut BTreeMap<usize, anyhow::Error>,
    index: usize,
    result: anyhow::Result<GitResponse>,
) {
    match result {
        Ok(output) => outputs.push(output),
        Err(error) => {
            transport_errors.insert(index, error);
        }
    }
}

impl ScriptedGitClient {
    fn respond(&self) -> anyhow::Result<GitResponse> {
        let index = self.invocations.fetch_add(1, Ordering::Relaxed);
        if let Some(error) = lock_or_recover(&self.transport_errors).remove(&index) {
            return Err(error);
        }
        lock_or_recover(&self.results)
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("scripted Git response {index} was not configured"))
    }

    fn repo_present(&self, repo_path: &Path) -> bool {
        !lock_or_recover(&self.absent_repos)
            .iter()
            .any(|p| p == repo_path)
    }
}

impl GitClient for ScriptedGitClient {
    fn repo_present(&self, repo_path: &RepositoryRoot) -> bool {
        self.repo_present(repo_path.as_ref())
    }
    fn probe_repository(&self, dir: &Path) -> anyhow::Result<GitRepositoryState> {
        if !self.repo_present(dir) {
            return Ok(GitRepositoryState::NotFound);
        }
        let Some(top_level) = scripted_capture(self, dir, &[])? else {
            return Ok(GitRepositoryState::NotARepository);
        };
        Ok(GitRepositoryState::Repository {
            top_level: RepositoryRoot::try_new(top_level.into())?,
        })
    }
    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<RepositoryRoot>> {
        scripted_capture(self, dir, &["rev-parse", "--show-toplevel"]).and_then(|top| {
            top.map(|path| RepositoryRoot::try_new(path.into()).map_err(Into::into))
                .transpose()
        })
    }
    fn top_level(&self, dir: &Path) -> anyhow::Result<RepositoryRoot> {
        scripted_capture(self, dir, &["rev-parse", "--show-toplevel"])?
            .map(|path| RepositoryRoot::try_new(path.into()))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("not a git repository"))
    }
    fn current_branch(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitHead> {
        scripted_capture(self, repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])?
            .map(|branch| git_head_fixture(&branch))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("not a git repository"))
    }
    fn upstream(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<GitRefName>> {
        match scripted_effect(
            self,
            repo_path,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            |output| -> anyhow::Result<GitRefName> {
                Ok(GitRefName::try_new(output.trim().to_owned())?)
            },
        )? {
            GitEffect::Applied(reference) => Ok(GitEffect::Applied(reference?)),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn branch_remote(
        &self,
        repo_path: &RepositoryRoot,
        branch: &BranchName,
    ) -> anyhow::Result<Option<RemoteName>> {
        scripted_capture(
            self,
            repo_path,
            &["config", &format!("branch.{branch}.remote")],
        )?
        .map(RemoteName::try_new)
        .transpose()
        .map_err(Into::into)
    }
    fn remote_url(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<Option<RemoteUrl>> {
        scripted_capture(self, repo_path, &["remote", "get-url", remote.as_ref()])?
            .map(RemoteUrl::try_new)
            .transpose()
            .map_err(Into::into)
    }
    fn remote_push_urls(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<Vec<RemoteUrl>> {
        scripted_capture(
            self,
            repo_path,
            &["remote", "get-url", "--push", "--all", remote.as_ref()],
        )?
        .into_iter()
        .flat_map(|urls| urls.lines().map(str::to_string).collect::<Vec<_>>())
        .map(RemoteUrl::try_new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
    }
    fn revision_exists(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<bool> {
        scripted_success(
            self,
            repo_path,
            &["rev-parse", "--verify", revision.as_ref()],
        )
    }
    fn commit_count(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Option<CommitCount>> {
        Ok(
            scripted_capture(self, repo_path, &["rev-list", "--count", range.as_ref()])?
                .and_then(|count| count.parse().ok())
                .map(CommitCount::new),
        )
    }
    fn ahead_behind(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Option<AheadBehind>> {
        Ok(scripted_capture(
            self,
            repo_path,
            &["rev-list", "--count", "--left-right", range.as_ref()],
        )?
        .and_then(|counts| {
            let (left, right) = counts.split_once(char::is_whitespace)?;
            Some(AheadBehind {
                behind: CommitCount::new(left.parse().ok()?),
                ahead: CommitCount::new(right.trim().parse().ok()?),
            })
        }))
    }
    fn working_tree(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<GitWorkingTree>> {
        match scripted_effect(
            self,
            repo_path,
            &["status", "--porcelain"],
            parse_working_tree,
        )? {
            GitEffect::Applied(tree) => Ok(GitEffect::Applied(tree?)),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn local_tags(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, gtl_models::tags::Tag>>> {
        match scripted_effect(
            self,
            repo_path,
            &["for-each-ref", tags::LOCAL_TAG_FORMAT_ARG, "refs/tags"],
            tags::parse_refs,
        )? {
            GitEffect::Applied(tags) => tags.map(GitEffect::Applied),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn remote_tags(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, GitObjectId>>> {
        scripted_effect(
            self,
            repo_path,
            &["ls-remote", "--tags", remote.as_ref()],
            tags::parse_remote_refs,
        )
    }
    fn stage_all(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(self, repo_path, &["add", "-A"], |_| ())
    }
    fn commit(
        &self,
        repo_path: &RepositoryRoot,
        message: &str,
    ) -> anyhow::Result<GitEffect<GitCommitReceipt>> {
        match scripted_effect(self, repo_path, &["commit", "-m", message], |output| {
            Ok::<_, CommitIdError>(GitCommitReceipt {
                detail: last_non_empty_line(output)
                    .unwrap_or("committed")
                    .to_string(),
                id: commit_identity(output).map_or_else(|| try_commit_id_fixture("c"), Ok)?,
            })
        })? {
            GitEffect::Applied(receipt) => Ok(GitEffect::Applied(receipt?)),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }
    fn fast_forward(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<String>> {
        scripted_effect(
            self,
            repo_path,
            &["merge", "--ff-only", revision.as_ref()],
            str::to_string,
        )
    }
    fn push_branch(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
        branch: &BranchName,
        mode: GitEffectMode,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>> {
        let mut args = vec!["push", remote.as_ref(), branch.as_ref()];
        if mode.is_dry_run() {
            args.push("--dry-run");
        }
        scripted_effect(self, repo_path, &args, |output| {
            push_receipt(output, "pushed")
        })
    }
    fn fetch(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<String>> {
        scripted_effect(self, repo_path, &["fetch", remote.as_ref()], str::to_string)
    }
    fn create_annotated_tag(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(
            self,
            repo_path,
            &["tag", "-a", tag.as_ref(), "-m", message],
            |_| (),
        )
    }
    fn create_annotated_tag_at(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        revision: &GitRevision,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(
            self,
            repo_path,
            &["tag", "-a", tag.as_ref(), revision.as_ref(), "-m", message],
            |_| (),
        )
    }
    fn create_lightweight_tag(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<()>> {
        scripted_effect(
            self,
            repo_path,
            &["tag", tag.as_ref(), &format!("{revision}^{{}}")],
            |_| (),
        )
    }
    fn push_tag_refs(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
        tags: &BTreeSet<GitRefName>,
    ) -> anyhow::Result<GitEffect<String>> {
        let mut owned = vec!["push".to_string(), remote.to_string()];
        owned.extend(
            tags.iter()
                .map(|reference| format!("{reference}:{reference}")),
        );
        let args = owned.iter().map(String::as_str).collect::<Vec<_>>();
        scripted_effect(self, repo_path, &args, str::to_string)
    }
    fn verify_commit(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<()> {
        scripted_success(
            self,
            repo_path,
            &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
        )?
        .then_some(())
        .ok_or_else(|| anyhow::anyhow!("not a commit"))
    }
    fn log_commits(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Vec<Commit>> {
        let Some(raw) = scripted_capture(
            self,
            repo_path,
            &[
                "log",
                "--format=%H%x1f%s%x1f%b%x1f%aI%x1f%P%x1e",
                range.as_ref(),
            ],
        )?
        else {
            return Ok(Vec::new());
        };
        raw.split('\u{1e}')
            .map(str::trim)
            .filter(|record| !record.is_empty())
            .map(|record| -> anyhow::Result<Commit> {
                let mut fields = record.trim().split('\u{1f}');
                let id = fields
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("scripted commit has no ID"))?;
                let subject = fields
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("scripted commit has no subject"))?;
                let body = fields
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("scripted commit has no body"))?;
                let committed_at = fields
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("scripted commit has no timestamp"))?;
                let parents = fields
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("scripted commit has no parents"))?;
                Ok(Commit {
                    id: id.trim().try_into()?,
                    subject: subject.trim().to_owned(),
                    body: body.trim().to_owned(),
                    committed_at: committed_at.trim().try_into()?,
                    parents: parents
                        .split_whitespace()
                        .map(TryInto::try_into)
                        .collect::<Result<Vec<_>, _>>()?,
                })
            })
            .collect()
    }
    fn diff(&self, _repo: &RepositoryRoot, _request: &GitDiffRequest) -> anyhow::Result<String> {
        anyhow::bail!("ScriptedGitClient does not implement diff output")
    }
    fn resolve_commit_id(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<CommitId> {
        scripted_capture(
            self,
            repo_path,
            &["rev-parse", &format!("{revision}^{{commit}}")],
        )?
        .map(|raw| try_commit_id_fixture(&raw))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("unknown revision"))
    }
    fn merge_base(
        &self,
        repo_path: &RepositoryRoot,
        left: &GitRevision,
        right: &GitRevision,
    ) -> anyhow::Result<CommitId> {
        scripted_capture(
            self,
            repo_path,
            &["merge-base", left.as_ref(), right.as_ref()],
        )?
        .map(|raw| try_commit_id_fixture(&raw))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("no merge base"))
    }
    fn committed_at(
        &self,
        _repo: &RepositoryRoot,
        _revision: &GitRevision,
    ) -> Option<MachineTimestamp> {
        None
    }
}

fn commit_identity(output: &str) -> Option<CommitId> {
    output.lines().rev().find_map(|line| {
        let line = strip_ansi_csi(line);
        let (header, subject) = line.trim().strip_prefix('[')?.split_once(']')?;
        if subject.trim().is_empty() {
            return None;
        }
        let identity = header.split_whitespace().next_back()?;
        try_commit_id_fixture(identity).ok()
    })
}

fn parse_working_tree(output: &str) -> anyhow::Result<GitWorkingTree> {
    let mut tree = GitWorkingTree::default();
    for line in output.lines().filter(|line| !line.is_empty()) {
        parse_working_tree_line(&mut tree, line)?;
    }
    Ok(tree)
}

fn parse_working_tree_line(tree: &mut GitWorkingTree, line: &str) -> anyhow::Result<()> {
    let bytes = line.as_bytes();
    if bytes.len() < 2 {
        return Ok(());
    }
    tree.files
        .push(gtl_models::repository::working_tree::CommitFile {
            status: line.get(0..2).unwrap_or("").trim().to_string(),
            path: RepositoryRelativePath::try_new(line.get(3..).unwrap_or("").into())?,
        });
    let (index, worktree) = (bytes[0], bytes[1]);
    if index == b'?' && worktree == b'?' {
        tree.unprepared.increment();
        return Ok(());
    }
    if index != b' ' {
        tree.staged.increment();
    }
    if worktree != b' ' {
        tree.unprepared.increment();
    }
    Ok(())
}

fn strip_ansi_csi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' || chars.next_if_eq(&'[').is_none() {
            output.push(character);
            continue;
        }
        let _ = chars
            .by_ref()
            .find(|control_character| ('@'..='~').contains(control_character));
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

#[cfg(test)]
pub(crate) fn make_repository(directory: &Path) {
    std::fs::create_dir_all(directory.join(".git")).unwrap();
}

#[cfg(test)]
pub(crate) fn make_linked_worktree(directory: &Path, administrative_directory: &Path) {
    std::fs::create_dir_all(administrative_directory).unwrap();
    std::fs::write(administrative_directory.join("commondir"), "../..\n").unwrap();
    make_git_file(directory, administrative_directory);
}

#[cfg(test)]
pub(crate) fn make_submodule(directory: &Path, module_directory: &Path) {
    std::fs::create_dir_all(module_directory).unwrap();
    make_git_file(directory, module_directory);
}

#[cfg(test)]
fn make_git_file(directory: &Path, git_directory: &Path) {
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(
        directory.join(".git"),
        format!("gitdir: {}\n", git_directory.display()),
    )
    .unwrap();
}

#[derive(Default)]
pub struct ProjectComparisons(
    pub  std::collections::BTreeMap<
        gtl_models::paths::RepositoryRoot,
        gtl_models::projects::comparison::ComparisonBranch,
    >,
);

impl crate::ports::ProjectComparisonReader for ProjectComparisons {
    fn comparison_branch(
        &self,
        path: &gtl_models::paths::RepositoryRoot,
    ) -> anyhow::Result<Option<gtl_models::projects::comparison::ComparisonBranch>> {
        Ok(self.0.get(path).cloned())
    }
}
