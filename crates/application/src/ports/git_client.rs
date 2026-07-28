use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use domain::{diffs::Commit, managed::working_tree::CommitFile, tags::Tag, worktrees::Worktree};

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

pub type BlameLines = HashMap<u32, String>;

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
    fn repo_present(&self, repo: &Path) -> bool;

    /// Classifies `dir` as a worktree repository, missing path, or non-repository path.
    fn probe_repository(&self, dir: &Path) -> anyhow::Result<GitRepositoryState>;

    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<PathBuf>>;

    /// Absolute path of the repository containing `dir`; errors when `dir` is not a git repo.
    fn top_level(&self, dir: &Path) -> anyhow::Result<String>;

    /// The current branch name (`HEAD`'s `--abbrev-ref`).
    fn current_branch(&self, repo: &Path) -> anyhow::Result<String>;

    /// The configured upstream tracking ref.
    fn upstream(&self, repo: &Path) -> anyhow::Result<GitEffect<String>>;

    fn branch_remote(&self, repo: &Path, branch: &str) -> anyhow::Result<Option<String>>;

    fn remote_url(&self, repo: &Path, remote: &str) -> anyhow::Result<Option<String>>;

    fn revision_exists(&self, repo: &Path, revision: &str) -> anyhow::Result<bool>;

    fn commit_count(&self, repo: &Path, range: &str) -> anyhow::Result<Option<usize>>;

    fn ahead_behind(&self, repo: &Path, range: &str) -> anyhow::Result<Option<(usize, usize)>>;

    fn is_ancestor(&self, repo: &Path, ancestor: &str, descendant: &str) -> anyhow::Result<bool>;

    fn working_tree(&self, repo: &Path) -> anyhow::Result<GitEffect<GitWorkingTree>>;

    fn merged_branches(
        &self,
        repo: &Path,
        into: &str,
    ) -> anyhow::Result<GitEffect<Vec<MergedBranch>>>;

    fn worktrees(&self, repo: &Path) -> anyhow::Result<GitEffect<Vec<Worktree>>>;

    fn local_tags(&self, repo: &Path) -> anyhow::Result<GitEffect<BTreeMap<String, Tag>>>;

    fn remote_tags(
        &self,
        repo: &Path,
        remote: &str,
    ) -> anyhow::Result<GitEffect<BTreeMap<String, String>>>;

    fn previous_checkout(&self, repo: &Path) -> anyhow::Result<Option<String>>;

    fn brief_log(&self, repo: &Path, range: &str) -> anyhow::Result<GitEffect<Vec<String>>>;

    fn diff_stat(
        &self,
        repo: &Path,
        before: &str,
        after: &str,
    ) -> anyhow::Result<GitEffect<String>>;

    fn stage_all(&self, repo: &Path) -> anyhow::Result<GitEffect<()>>;

    fn commit(&self, repo: &Path, message: &str) -> anyhow::Result<GitEffect<GitCommitReceipt>>;

    fn switch(&self, repo: &Path, branch: &str) -> anyhow::Result<GitEffect<()>>;

    fn switch_previous(&self, repo: &Path) -> anyhow::Result<GitEffect<()>>;

    fn fast_forward(&self, repo: &Path, revision: &str) -> anyhow::Result<GitEffect<String>>;

    fn move_branch(
        &self,
        repo: &Path,
        branch: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>>;

    fn delete_branch(&self, repo: &Path, branch: &str) -> anyhow::Result<GitEffect<()>>;

    fn soft_reset(&self, repo: &Path, revision: &str) -> anyhow::Result<GitEffect<()>>;

    fn push_branch(
        &self,
        repo: &Path,
        remote: &str,
        branch: &str,
        dry_run: bool,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>>;

    fn fetch(&self, repo: &Path, remote: &str) -> anyhow::Result<GitEffect<String>>;

    fn create_annotated_tag(
        &self,
        repo: &Path,
        tag: &str,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>>;

    fn create_lightweight_tag(
        &self,
        repo: &Path,
        tag: &str,
        revision: &str,
    ) -> anyhow::Result<GitEffect<()>>;

    fn push_tag_refs(
        &self,
        repo: &Path,
        remote: &str,
        tags: &[String],
    ) -> anyhow::Result<GitEffect<String>>;

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

    /// A unified diff or changed-path listing for a revision range.
    fn diff(&self, repo: &Path, request: &GitDiffRequest) -> anyhow::Result<String>;

    /// Range-bounded forward blame porcelain of `path` at `base..tip`.
    fn blame_forward(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<BlameLines>;

    /// Working-tree forward blame porcelain of `path` (hash mode: base -> worktree).
    fn blame_forward_worktree(&self, repo: &Path, path: &str) -> anyhow::Result<BlameLines>;

    /// Reverse blame porcelain of `path` at `base..tip` (each deleted line carries its deleter).
    fn blame_reverse(
        &self,
        repo: &Path,
        base: &str,
        tip: &str,
        path: &str,
    ) -> anyhow::Result<BlameLines>;

    /// The repo's stable oldest root-commit sha, or `None` for a repo with no commits.
    fn root_commit(&self, repo: &Path) -> Option<String>;

    /// Resolve `rev` to its full 40-char sha.
    fn resolve_sha(&self, repo: &Path, rev: &str) -> anyhow::Result<String>;

    /// The best common ancestor of two revisions.
    fn merge_base(&self, repo: &Path, left: &str, right: &str) -> anyhow::Result<String>;

    /// The committer date of `rev` as a display string; never fails (empty on error).
    fn committed_at(&self, repo: &Path, rev: &str) -> String;
}
