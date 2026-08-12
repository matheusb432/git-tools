use std::path::{Path, PathBuf};

/// Filesystem discovery of git repos under a root. The walk - and its prune/skip
/// decisions, via `crate::discovery::rules` - lives in the infra adapter; the
/// `discovery::find_repos` slice labels the results.
pub trait RepoDiscovery: Clone + Send + Sync + 'static {
    /// Every git repo directory under `root`, sorted. Linked worktrees (and their
    /// subtrees) are skipped unless `include_worktrees`; VCS-internal and
    /// build/dependency directories are always pruned.
    fn find_repos(&self, root: &Path, include_worktrees: bool) -> anyhow::Result<Vec<PathBuf>>;
}
