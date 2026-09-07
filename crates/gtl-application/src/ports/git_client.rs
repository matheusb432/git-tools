use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use gtl_models::{
    diffs::{Commit, CommitId},
    git::{
        AheadBehind, BranchName, CommitCount, GitDiffSpec, GitEffectMode, GitHead, GitObjectId,
        GitRange, GitRefName, GitRevision, RemoteName, RemoteUrl, TagName,
    },
    paths::{RepositoryRelativePath, RepositoryRoot},
    repository::{PathCount, working_tree::CommitFile},
    tags::Tag,
    timestamps::MachineTimestamp,
    worktrees::Worktree,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDiffFormat {
    NamesOnly,
    Unified,
    FullContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiffRequest {
    pub spec: GitDiffSpec,
    pub format: GitDiffFormat,
    pub excluded_paths: Vec<RepositoryRelativePath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitEffect<T> {
    Applied(T),
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommitReceipt {
    pub detail: String,
    pub id: CommitId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitPushReceipt {
    UpToDate { detail: String },
    Updated { detail: String },
}

impl GitPushReceipt {
    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Self::UpToDate { detail } | Self::Updated { detail } => detail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitWorkingTree {
    pub files: Vec<CommitFile>,
    pub staged: PathCount,
    pub unprepared: PathCount,
}

/// Whether a completed repository snapshot found working-tree changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitWorkingTreeSummary {
    /// No staged, unstaged, or untracked files were found.
    Clean,
    /// At least one staged, unstaged, or untracked file was found.
    Dirty,
}

impl GitWorkingTreeSummary {
    /// Collapses a completed working-tree snapshot to clean or dirty.
    #[must_use]
    pub fn from_working_tree(working_tree: &GitWorkingTree) -> Self {
        if working_tree.files.is_empty() {
            Self::Clean
        } else {
            Self::Dirty
        }
    }

    /// Returns whether the snapshot contained any working-tree changes.
    #[must_use]
    pub const fn is_dirty(self) -> bool {
        matches!(self, Self::Dirty)
    }
}

/// Upstream facts collected with a repository status snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusUpstream {
    pub reference: GitRefName,
    pub ahead: CommitCount,
}

/// Branch, upstream, and working-tree facts collected by one Git status operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusSnapshot {
    pub head: GitHead,
    pub upstream: Option<GitStatusUpstream>,
    pub working_tree: GitWorkingTree,
}

/// Classification of a path as a working-tree repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRepositoryState {
    /// A repository, identified by its canonical top-level directory.
    Repository { top_level: RepositoryRoot },
    /// The candidate directory does not exist.
    NotFound,
    /// The directory exists but is not inside a Git worktree.
    NotARepository,
}

/// Git repository effects used by application operations.
pub trait GitClient: Clone + Send + Sync + 'static {
    fn repo_present(&self, repo_path: &RepositoryRoot) -> bool;

    /// Classifies `dir` as a worktree repository, missing path, or non-repository path.
    fn probe_repository(&self, dir: &Path) -> anyhow::Result<GitRepositoryState>;

    /// Returns the canonical worktree root containing `dir`, or `None` outside a repository.
    fn discover_top(&self, dir: &Path) -> anyhow::Result<Option<RepositoryRoot>>;

    /// Absolute path of the repository containing `dir`; errors when `dir` is not a git repo.
    fn top_level(&self, dir: &Path) -> anyhow::Result<RepositoryRoot>;

    /// The current branch name (`HEAD`'s `--abbrev-ref`).
    fn current_branch(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitHead>;

    fn head_state(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<gtl_models::git::GitHeadState> {
        Ok(gtl_models::git::GitHeadState::Commit {
            head: self.current_branch(repo_path)?,
            id: self.resolve_commit_id(repo_path, &GitRevision::head())?,
        })
    }

    /// The configured upstream tracking ref.
    fn upstream(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<GitRefName>>;

    fn branch_remote(
        &self,
        repo_path: &RepositoryRoot,
        branch: &BranchName,
    ) -> anyhow::Result<Option<RemoteName>>;

    fn remote_url(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<Option<RemoteUrl>>;

    fn revision_exists(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<bool>;

    fn commit_count(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Option<CommitCount>>;

    /// Reads the facts needed to classify one repository status.
    fn status_snapshot(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<GitStatusSnapshot>> {
        let head = self.current_branch(repo_path)?;
        let upstream = match &head {
            GitHead::Detached => None,
            GitHead::Branch(_) => match self.upstream(repo_path) {
                Ok(GitEffect::Applied(reference)) => Some(GitStatusUpstream {
                    reference,
                    ahead: self
                        .commit_count(repo_path, &GitRange::upstream_to_head())
                        .ok()
                        .flatten()
                        .unwrap_or_default(),
                }),
                Ok(GitEffect::Rejected(_)) | Err(_) => None,
            },
        };
        match self.working_tree(repo_path)? {
            GitEffect::Applied(working_tree) => Ok(GitEffect::Applied(GitStatusSnapshot {
                head,
                upstream,
                working_tree,
            })),
            GitEffect::Rejected(detail) => Ok(GitEffect::Rejected(detail)),
        }
    }

    /// Reads one status snapshot while allowing adapters to reuse completed descendant states.
    fn status_snapshot_with_known_descendants(
        &self,
        repo_path: &RepositoryRoot,
        _known_descendants: &BTreeMap<RepositoryRoot, GitWorkingTreeSummary>,
    ) -> anyhow::Result<GitEffect<GitStatusSnapshot>> {
        self.status_snapshot(repo_path)
    }

    fn ahead_behind(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Option<AheadBehind>>;

    fn working_tree(&self, repo_path: &RepositoryRoot)
    -> anyhow::Result<GitEffect<GitWorkingTree>>;

    fn worktrees(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<Vec<Worktree>>>;

    fn local_tags(
        &self,
        repo_path: &RepositoryRoot,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, Tag>>>;

    fn remote_tags(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<BTreeMap<TagName, GitObjectId>>>;

    fn stage_all(&self, repo_path: &RepositoryRoot) -> anyhow::Result<GitEffect<()>>;

    fn commit(
        &self,
        repo_path: &RepositoryRoot,
        message: &str,
    ) -> anyhow::Result<GitEffect<GitCommitReceipt>>;

    fn fast_forward(
        &self,
        repo_path: &RepositoryRoot,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<String>>;

    fn push_branch(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
        branch: &BranchName,
        mode: GitEffectMode,
    ) -> anyhow::Result<GitEffect<GitPushReceipt>>;

    fn fetch(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
    ) -> anyhow::Result<GitEffect<String>>;

    fn create_annotated_tag(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>>;

    /// Creates an annotated tag at an explicit revision.
    ///
    /// Existing adapters safely reject non-`HEAD` revisions until they override this method.
    fn create_annotated_tag_at(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        revision: &GitRevision,
        message: &str,
    ) -> anyhow::Result<GitEffect<()>> {
        if revision.as_ref() == "HEAD" {
            self.create_annotated_tag(repo_path, tag, message)
        } else {
            Ok(GitEffect::Rejected(
                "Git client cannot create an annotated tag at an explicit revision".into(),
            ))
        }
    }

    fn create_lightweight_tag(
        &self,
        repo_path: &RepositoryRoot,
        tag: &TagName,
        revision: &GitRevision,
    ) -> anyhow::Result<GitEffect<()>>;

    fn push_tag_refs(
        &self,
        repo_path: &RepositoryRoot,
        remote: &RemoteName,
        tags: &BTreeSet<GitRefName>,
    ) -> anyhow::Result<GitEffect<String>>;

    /// Confirm `rev` resolves to a commit; errors when it does not.
    fn verify_commit(&self, repo_path: &RepositoryRoot, rev: &GitRevision) -> anyhow::Result<()>;

    /// The commits in `range`, newest first.
    fn log_commits(
        &self,
        repo_path: &RepositoryRoot,
        range: &GitRange,
    ) -> anyhow::Result<Vec<Commit>>;

    /// A unified diff or changed-path listing for a revision range.
    fn diff(&self, repo_path: &RepositoryRoot, request: &GitDiffRequest) -> anyhow::Result<String>;

    /// Resolves `rev` to its full validated commit ID.
    fn resolve_commit_id(
        &self,
        repo_path: &RepositoryRoot,
        rev: &GitRevision,
    ) -> anyhow::Result<CommitId>;

    /// The best common ancestor of two revisions.
    fn merge_base(
        &self,
        repo_path: &RepositoryRoot,
        left: &GitRevision,
        right: &GitRevision,
    ) -> anyhow::Result<CommitId>;

    /// The committer timestamp of `rev`; returns `None` when Git or decoding fails.
    fn committed_at(
        &self,
        repo_path: &RepositoryRoot,
        rev: &GitRevision,
    ) -> Option<MachineTimestamp>;
}
