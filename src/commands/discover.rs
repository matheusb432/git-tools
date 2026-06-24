//! Filesystem discovery of git repositories under a root directory.
//!
//! A focused, diff-agnostic concern shared by `diff subrepos` and recursive `status`:
//! walk a tree, collect real repos, and (optionally) skip linked worktrees.

use std::path::{Path, PathBuf};

use anyhow::Context;
use walkdir::{DirEntry, WalkDir};

/// Collect every git repo under `root`, sorted by path. Linked worktrees (and their
/// subtrees) are skipped unless `include_worktrees` is set; VCS-internal and
/// build/dependency directories are always pruned (see [`PRUNED_DIRS`]).
pub(super) fn discover_git_repos(
    root: &Path,
    include_worktrees: bool,
) -> anyhow::Result<Vec<PathBuf>> {
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

/// Directory names never worth descending into: VCS internals and build/dependency
/// output. They hold no status-worthy repo yet can dwarf the rest of the tree (a
/// single `target/` is thousands of dirs), so pruning them keeps the scan O(repo)
/// instead of O(every file). A repo literally named one of these is skipped — an
/// acceptable trade for not walking entire build trees.
const PRUNED_DIRS: &[&str] = &[".git", "target", "node_modules"];

/// Whether `walkdir` should skip an entry — and, for a directory, its whole subtree.
fn is_skipped(entry: &DirEntry, include_worktrees: bool) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    // ! A linked worktree carries a full working tree of its own; skipping it (and its
    // ! subtree) by default keeps the scan to real, independent repos.
    if !include_worktrees && is_linked_worktree(entry.path()) {
        return true;
    }
    // The root (depth 0) is always scanned, even when its own name is in PRUNED_DIRS.
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| PRUNED_DIRS.contains(&name))
}

// ! A linked worktree's `.git` is a file (not a dir) whose `gitdir:` points into another
// ! repo's `.git/worktrees/<name>` — that pointer is git's own marker, robust to layout.
fn is_linked_worktree(dir: &Path) -> bool {
    let git_path = dir.join(".git");
    if !git_path.is_file() {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&git_path) else {
        return false;
    };
    content
        .lines()
        .filter_map(|line| line.strip_prefix("gitdir:"))
        .any(|target| target.trim().replace('\\', "/").contains("/worktrees/"))
}

/// Label a discovered repo by its path relative to `root`, falling back to the repo's
/// own directory name when it *is* the root.
pub(super) fn repo_label(root: &Path, repo: &Path) -> String {
    let relative = repo.strip_prefix(root).unwrap_or(repo);
    let label = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if label.is_empty() {
        super::repo_name(repo)
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn make_repo(dir: &Path) {
        fs::create_dir_all(dir.join(".git")).unwrap();
    }

    fn make_worktree(dir: &Path, gitdir: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(".git"), format!("gitdir: {gitdir}\n")).unwrap();
    }

    #[test]
    fn is_linked_worktree_detects_worktree_pointer() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        make_worktree(&wt, "/repo/.git/worktrees/feature");
        assert!(is_linked_worktree(&wt));
    }

    #[test]
    fn is_linked_worktree_ignores_plain_repo_and_submodule() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        make_repo(&repo);
        assert!(!is_linked_worktree(&repo));

        let submodule = tmp.path().join("sub");
        make_worktree(&submodule, "/repo/.git/modules/sub");
        assert!(!is_linked_worktree(&submodule));
    }

    #[test]
    fn discover_skips_nested_worktrees_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("api"));
        make_worktree(
            &root.join("api/.worktrees/feature"),
            "/abs/api/.git/worktrees/feature",
        );

        let repos = discover_git_repos(root, false).unwrap();
        assert_eq!(repos, vec![root.join("api")]);
    }

    #[test]
    fn discover_prunes_build_and_dependency_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("app"));
        make_repo(&root.join("app/libs/inner")); // real nested subrepo — kept
        make_repo(&root.join("app/target/some/dep")); // cargo build output — pruned
        make_repo(&root.join("app/node_modules/pkg")); // dependency — pruned

        let repos = discover_git_repos(root, false).unwrap();

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
    fn discover_includes_worktrees_with_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        make_repo(&root.join("api"));
        let wt = root.join("api/.worktrees/feature");
        make_worktree(&wt, "/abs/api/.git/worktrees/feature");

        let repos = discover_git_repos(root, true).unwrap();
        assert!(repos.contains(&root.join("api")));
        assert!(repos.contains(&wt));
    }
}
