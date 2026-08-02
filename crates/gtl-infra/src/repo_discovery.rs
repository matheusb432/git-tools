//! `WalkdirRepoDiscovery`: the [`RepoDiscovery`] adapter — a `walkdir` tree walk
//! that prunes/skips via the pure `gtl_application::discovery::rules`, reading each
//! candidate's `.git` file to classify linked worktrees. The imperative shell over
//! the pure decision core.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gtl_application::{discovery::rules, ports::RepoDiscovery};
use walkdir::{DirEntry, WalkDir};

/// Discovers git repos under a root with `walkdir`.
#[derive(Debug, Clone, Copy, Default)]
pub struct WalkdirRepoDiscovery;

impl RepoDiscovery for WalkdirRepoDiscovery {
    fn find_repos(&self, root: &Path, include_worktrees: bool) -> anyhow::Result<Vec<PathBuf>> {
        let mut repos = Vec::new();
        let walk = WalkDir::new(root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| !is_skipped(entry, include_worktrees));
        for entry in walk {
            let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
            if entry.file_type().is_dir() && entry.path().join(".git").exists() {
                repos.push(entry.into_path());
            }
        }
        repos.sort();
        Ok(repos)
    }
}

/// Whether `walkdir` should skip an entry (and, for a directory, its subtree),
/// delegating the decision to the pure `gtl_application::discovery::rules`.
fn is_skipped(entry: &DirEntry, include_worktrees: bool) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_str().unwrap_or_default();
    let is_worktree = is_linked_worktree(entry.path());
    rules::should_skip(entry.depth(), name, is_worktree, include_worktrees)
}

/// Reads a directory's `.git` file (if it is a file) and classifies it via the
/// pure marker rule. A missing/dir `.git` or a read error is "not a worktree".
fn is_linked_worktree(dir: &Path) -> bool {
    let git_path = dir.join(".git");
    if !git_path.is_file() {
        return false;
    }
    std::fs::read_to_string(&git_path).is_ok_and(|content| rules::is_worktree_marker(&content))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::*;

    fn make_repo(dir: &Path) {
        fs::create_dir_all(dir.join(".git")).unwrap();
    }

    fn make_worktree(dir: &Path, gitdir: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(".git"), format!("gitdir: {gitdir}\n")).unwrap();
    }

    #[test]
    fn skips_nested_worktrees_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("api"));
        make_worktree(
            &root.join("api/.worktrees/feature"),
            "/abs/api/.git/worktrees/feature",
        );

        let repos = WalkdirRepoDiscovery.find_repos(root, false).unwrap();
        assert_eq!(repos, vec![root.join("api")]);
    }

    #[test]
    fn includes_worktrees_with_the_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("api"));
        let wt = root.join("api/.worktrees/feature");
        make_worktree(&wt, "/abs/api/.git/worktrees/feature");

        let repos = WalkdirRepoDiscovery.find_repos(root, true).unwrap();
        assert!(repos.contains(&root.join("api")));
        assert!(repos.contains(&wt));
    }

    #[test]
    fn prunes_build_and_dependency_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("app"));
        make_repo(&root.join("app/libs/inner")); // real nested subrepo — kept
        make_repo(&root.join("app/target/some/dep")); // build output — pruned
        make_repo(&root.join("app/node_modules/pkg")); // dependency — pruned

        let repos = WalkdirRepoDiscovery.find_repos(root, false).unwrap();

        assert!(repos.contains(&root.join("app")));
        assert!(repos.contains(&root.join("app/libs/inner")));
        assert!(
            !repos.iter().any(|p| p.components().any(|c| {
                let name = c.as_os_str();
                name == "target" || name == "node_modules"
            })),
            "build/dependency dirs should be pruned, got: {repos:?}"
        );
    }

    #[test]
    fn ignores_a_submodule_git_pointer() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("api"));
        // A submodule's `.git` points into modules/, not worktrees/ — treated as a repo dir.
        make_worktree(&root.join("sub"), "/repo/.git/modules/sub");

        let repos = WalkdirRepoDiscovery.find_repos(root, false).unwrap();
        assert!(repos.contains(&root.join("sub")));
    }
}
