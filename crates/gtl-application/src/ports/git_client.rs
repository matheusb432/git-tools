use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use gtl_models::{
    diffs::Commit, managed::working_tree::CommitFile, tags::Tag, worktrees::Worktree,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDiffFormat {
    NamesOnly,
    Unified,
    FullContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiffRequest {
    pub range: String,
    pub format: GitDiffFormat,
    pub excluded_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitEffect<T> {
    Applied(T),
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommitReceipt {
    pub detail: String,
    pub identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitPushReceipt {
    pub detail: String,
    pub up_to_date: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitWorkingTree {
    pub files: Vec<CommitFile>,
    pub staged: usize,
    pub unprepared: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedBranch {
    pub name: String,
    pub sha: String,
}

/// Classification of a path as a working-tree repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRepositoryState {
    /// A repository, identified by its canonical top-level directory.
    Repository { top_level: PathBuf },
    /// The candidate directory does not exist.
    NotFound,
    /// The directory exists but is not inside a Git worktree.
    NotARepository,
}

/// Git repository effects used by application operations.
pub trait GitClient: Clone + Send + Sync + 'static {
    fn repo_present(&self, repo_path: &Path) -> bool;

    /// Classifies `dir` as a worktree repository, missing path, or non-repository path.
    fn probe_repository(&self, dir: &Path) -> anyhow::Result<GitRepositoryState>;

    /// Returns the canonical worktree root containing `dir`, or `None` outside a repository.
    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<PathBuf>>;

    /// Absolute path of the repository containing `dir`; errors when `dir` is not a git repo.
    fn top_level(&self, dir: &Path) -> anyhow::Result<String>;

    /// The current branch name (`HEAD`'s `--abbrev-ref`).
    fn current_branch(&self, repo_path: &Path) -> anyhow::Result<String>;

    /// The configured upstream tracking ref.
    fn upstream(&self, repo_path: &Path) -> anyhow::Result<GitEffect<String>>;

    fn branch_remote(&self, repo_path: &Path, branch: &str) -> anyhow::Result<Option<String>>;

    fn remote_url(&self, repo_path: &Path, remote: &str) -> anyhow::Result<Option<String>>;

    fn revision_exists(&self, repo_path: &Path, revision: &str) -> anyhow::Result<bool>;

    fn commit_count(&self, repo_path: &Path, range: &str) -> anyhow::Result<Option<usize>>;

    fn ahead_behind(&self, repo_path: &Path, range: &str)
    -> anyhow::Result<Option<(usize, usize)>>;

    fn is_ancestor(
        &self,
        repo_path: &Path,
        ancestor: &str,
        descendant: &str,
    ) -> anyhow::Result<bool>;

    fn working_tree(&self, repo_path: &Path) -> anyhow::Result<GitEffect<GitWorkingTree>>;

    fn merged_branches(
        &self,
        repo_path: &Path,
        into: &str,
    ) -> anyhow::Result<GitEffect<Vec<MergedBranch>>>;

    fn worktrees(&self, repo_path: &Path) -> anyhow::Result<GitEffect<Vec<Worktree>>>;

    fn local_tags(&self, repo_path: &Path) -> anyhow::Result<GitEffect<BTreeMap<String, Tag>>>;

    fn remote_tags(
        &self,
        repo_path: &Path,
        remote: &str,
    ) -> anyhow::Result<GitEffect<BTreeMap<String, String>>>;

    fn previous_checkout(&self, repo_path: &Path) -> anyhow::Result<Option<String>>;

    fn brief_log(&self, repo_path: &Path, range: &str) -> anyhow::Result<GitEffect<Vec<String>>>;

    fn diff_stat(
        &self,
        repo_path: &Path,
        before: &str,
        after: &str,
    ) -> anyhow::Result<GitEffect<String>>;

    fn stage_all(&self, repo_path: &Path) -> anyhow::Result<GitEffect<()>>;

    fn commit(
        &self,
        repo_path: &Path,
        message: &str,
    ) -> anyhow::Result<GitEffect<GitCommitReceipt>>;

    fn switch(&self, repo_path: &Path, branch: &str) -> anyhow::Result<GitEffect<()>>;

    fn switch_previous(&self, repo_path: &Path) -> anyhow::Result<GitEffect<()>>;

    fn fast_forward(&self, repo_path: &Path, revision: &str) -> anyhow::Result<GitEffect<String>>;

    fn move_branch(
        &self,
        repo_path: &Path,
        branch: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>>;

    fn delete_branch(&self, repo_path: &Path, branch: &str) -> anyhow::Result<GitEffect<()>>;

    fn soft_reset(&self, repo_path: &Path, revision: &str) -> anyhow::Result<GitEffect<()>>;

    fn push_branch(
        &self,
        repo_path: &Path,
        remote: &str,
        branch: &str,
        dry_run: bool,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>>;

    fn fetch(&self, repo_path: &Path, remote: &str) -> anyhow::Result<GitEffect<String>>;

    fn create_annotated_tag(
        &self,
        repo_path: &Path,
        tag: &str,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>>;

    /// Creates an annotated tag at an explicit revision.
    ///
    /// Existing adapters safely reject non-`HEAD` revisions until they override this method.
    fn create_annotated_tag_at(
        &self,
        repo_path: &Path,
        tag: &str,
        revision: &str,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        if revision == "HEAD" {
            self.create_annotated_tag(repo_path, tag, message)
        } else {
            Ok(GitEffect::Rejected(
                "Git client cannot create an annotated tag at an explicit revision".into(),
            ))
        }
    }

    fn create_lightweight_tag(
        &self,
        repo_path: &Path,
        tag: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>>;

    fn push_tag_refs(
        &self,
        repo_path: &Path,
        remote: &str,
        tags: &[String],
    ) -> anyhow::Result<GitEffect<String>>;

    /// Confirm `rev` resolves to a commit; errors when it does not.
    fn verify_commit(&self, repo_path: &Path, rev: &str) -> anyhow::Result<()>;

    /// The abbreviated sha for `rev`.
    fn short_ref(&self, repo_path: &Path, rev: &str) -> anyhow::Result<String>;

    /// The commits in `range`, newest first.
    fn log_commits(&self, repo_path: &Path, range: &str) -> anyhow::Result<Vec<Commit>>;

    /// A unified diff or changed-path listing for a revision range.
    fn diff(&self, repo_path: &Path, request: &GitDiffRequest) -> anyhow::Result<String>;

    /// The repo's stable oldest root-commit sha, or `None` for a repo with no commits.
    fn root_commit(&self, repo_path: &Path) -> Option<String>;

    /// Resolve `rev` to its full 40-char sha.
    fn resolve_sha(&self, repo_path: &Path, rev: &str) -> anyhow::Result<String>;

    /// The best common ancestor of two revisions.
    fn merge_base(&self, repo_path: &Path, left: &str, right: &str) -> anyhow::Result<String>;

    /// The committer date of `rev` as a display string; never fails (empty on error).
    fn committed_at(&self, repo_path: &Path, rev: &str) -> String;
}
