use gtl_models::{
    diffs::PinnedRange,
    git::{GitRange, GitRevision},
    paths::RepositoryRoot,
};

use crate::ports::GitClient;

pub(crate) const DEFAULT_MERGE_BASE: &str = "main";

pub(crate) fn resolve_range(
    repo_path: &RepositoryRoot,
    base: &GitRevision,
    head: &GitRevision,
    git: &impl GitClient,
) -> Option<PinnedRange> {
    Some(PinnedRange {
        base: git.resolve_commit_id(repo_path, base).ok()?,
        head: git.resolve_commit_id(repo_path, head).ok()?,
    })
}

pub(crate) fn resolve_exact_range(
    repo_path: &RepositoryRoot,
    range: &GitRange,
    git: &impl GitClient,
) -> Option<PinnedRange> {
    if range.as_ref().contains("...") {
        return None;
    }
    let (base, head) = range.as_ref().split_once("..")?;
    if base.is_empty() || head.is_empty() {
        return None;
    }
    resolve_range(
        repo_path,
        &GitRevision::try_new(base.to_owned()).ok()?,
        &GitRevision::try_new(head.to_owned()).ok()?,
        git,
    )
}

pub(crate) fn resolve_merge_range(
    repo_path: &RepositoryRoot,
    base: Option<&GitRevision>,
    git: &impl GitClient,
) -> Option<PinnedRange> {
    let base = base.cloned().unwrap_or_else(GitRevision::main);
    let head = GitRevision::head();
    Some(PinnedRange {
        base: git.merge_base(repo_path, &base, &head).ok()?,
        head: git.resolve_commit_id(repo_path, &head).ok()?,
    })
}
