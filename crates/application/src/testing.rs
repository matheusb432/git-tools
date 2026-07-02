//! In-memory fakes for the application ports (template pattern: cheap-clone
//! shared state so tests keep a handle after passing a fake in).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use domain::diffs::{Commit, DiffKind};

use crate::ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer, PlacedArtifact};

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
            .map(|o| o.commits.clone())
            .unwrap_or_else(|| self.commits.clone()))
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
            .map(|o| o.diff_output.clone())
            .unwrap_or_else(|| self.diff_output.clone()))
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

/// Recording `ArtifactStore`: `place` records and returns a deterministic path;
/// `lookup_by_range` answers from the `range_hits` script.
#[derive(Debug, Default, Clone)]
pub struct InMemoryArtifactStore {
    pub placed: Arc<Mutex<Vec<(ArtifactMeta, String)>>>,
    pub range_hits: Arc<Mutex<HashMap<(DiffKind, String, String), PathBuf>>>,
}

impl ArtifactStore for InMemoryArtifactStore {
    fn place(
        &self,
        store_root: &Path,
        meta: &ArtifactMeta,
        html: &str,
    ) -> anyhow::Result<PlacedArtifact> {
        self.placed
            .lock()
            .unwrap()
            .push((meta.clone(), html.to_string()));
        Ok(PlacedArtifact {
            path: store_root.join("diffs/fake/artifact.html"),
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
