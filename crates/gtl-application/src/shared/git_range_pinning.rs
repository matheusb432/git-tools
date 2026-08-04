use std::path::Path;

use crate::ports::GitClient;

pub(crate) const DEFAULT_MERGE_BASE: &str = "main";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedGitRange {
    pub(crate) base: String,
    pub(crate) head: String,
}

pub(crate) fn resolve_range(
    repo_path: &Path,
    base: &str,
    head: &str,
    git: &impl GitClient,
) -> Option<ResolvedGitRange> {
    Some(ResolvedGitRange {
        base: git.resolve_sha(repo_path, base).ok()?,
        head: git.resolve_sha(repo_path, head).ok()?,
    })
}

pub(crate) fn resolve_exact_range(
    repo_path: &Path,
    range: &str,
    git: &impl GitClient,
) -> Option<ResolvedGitRange> {
    if range.contains("...") {
        return None;
    }
    let (base, head) = range.split_once("..")?;
    if base.is_empty() || head.is_empty() {
        return None;
    }
    resolve_range(repo_path, base, head, git)
}

pub(crate) fn resolve_merge_range(
    repo_path: &Path,
    base: Option<&str>,
    git: &impl GitClient,
) -> Option<ResolvedGitRange> {
    let base = base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_MERGE_BASE);
    Some(ResolvedGitRange {
        base: git.merge_base(repo_path, base, "HEAD").ok()?,
        head: git.resolve_sha(repo_path, "HEAD").ok()?,
    })
}
