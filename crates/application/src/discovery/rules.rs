//! Pure discovery rules: prune policy, worktree-marker classification, the skip
//! decision, and repo labeling. Values in, decision out — no I/O. The infra walk
//! adapter calls these during traversal; the query slice labels with them.

use std::path::Path;

/// Directory names never worth descending into: VCS internals and build/dependency
/// output. A single `target/` is thousands of dirs and holds no status-worthy repo,
/// so pruning them keeps a scan O(repos) instead of O(every file). A repo literally
/// named one of these is skipped — an accepted trade for not walking build trees.
pub const PRUNED_DIRS: &[&str] = &[".git", "target", "node_modules"];

/// Whether `name` is a pruned directory name (see [`PRUNED_DIRS`]).
pub fn is_pruned_dir(name: &str) -> bool {
    PRUNED_DIRS.contains(&name)
}

/// Whether the contents of a `.git` *file* mark a linked worktree. A linked
/// worktree's `.git` is a file whose `gitdir:` points into another repo's
/// `.git/worktrees/<name>`; a submodule points into `.git/modules/<name>` and is
/// not a worktree. Pure over the already-read file contents.
pub fn is_worktree_marker(git_file_contents: &str) -> bool {
    git_file_contents
        .lines()
        .filter_map(|line| line.strip_prefix("gitdir:"))
        .any(|target| target.trim().replace('\\', "/").contains("/worktrees/"))
}

/// Whether the walk should skip a directory entry — and, for a directory, its whole
/// subtree. Pure over facts the adapter resolves:
/// - depth 0 (the root) is always scanned, even when named like a pruned dir;
/// - a linked worktree is skipped unless `include_worktrees`;
/// - otherwise a pruned directory name is skipped.
pub fn should_skip(
    depth: usize,
    name: &str,
    is_linked_worktree: bool,
    include_worktrees: bool,
) -> bool {
    if !include_worktrees && is_linked_worktree {
        return true;
    }
    depth > 0 && is_pruned_dir(name)
}

/// Label a discovered repo by its path relative to `root`, falling back to the
/// repo's own directory name when it *is* the root.
pub fn repo_label(root: &Path, repo: &Path) -> String {
    let relative = repo.strip_prefix(root).unwrap_or(repo);
    let label = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if label.is_empty() {
        repo_name(repo)
    } else {
        label
    }
}

/// The repo's directory name, or `"repo"` for a nameless path.
pub fn repo_name(top: &Path) -> String {
    top.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn prunes_vcs_and_build_dirs_only() {
        assert!(is_pruned_dir("target"));
        assert!(is_pruned_dir("node_modules"));
        assert!(is_pruned_dir(".git"));
        assert!(!is_pruned_dir("src"));
        assert!(!is_pruned_dir(""));
    }

    #[test]
    fn worktree_marker_detects_worktree_pointer_only() {
        assert!(is_worktree_marker("gitdir: /repo/.git/worktrees/feature\n"));
        // A submodule points into modules/, not worktrees/ — not a worktree.
        assert!(!is_worktree_marker("gitdir: /repo/.git/modules/sub\n"));
        // Windows-style separators normalize.
        assert!(is_worktree_marker("gitdir: C:\\repo\\.git\\worktrees\\f\n"));
        assert!(!is_worktree_marker("ref: refs/heads/main\n"));
    }

    #[test]
    fn should_skip_always_scans_the_root_even_when_named_like_a_pruned_dir() {
        assert!(!should_skip(0, "target", false, false));
    }

    #[test]
    fn should_skip_prunes_build_dirs_below_the_root() {
        assert!(should_skip(1, "target", false, false));
        assert!(!should_skip(1, "src", false, false));
    }

    #[test]
    fn should_skip_honors_the_worktree_flag() {
        assert!(should_skip(2, "feature", true, false));
        assert!(!should_skip(2, "feature", true, true));
    }

    #[test]
    fn repo_label_is_relative_to_root_and_falls_back_to_dir_name() {
        let root = Path::new("/work");
        assert_eq!(repo_label(root, Path::new("/work/api")), "api");
        assert_eq!(
            repo_label(root, Path::new("/work/libs/inner")),
            "libs/inner"
        );
        // repo == root: use the root's own directory name.
        assert_eq!(repo_label(root, root), "work");
    }

    #[test]
    fn repo_name_uses_the_final_component_or_a_fallback() {
        assert_eq!(repo_name(Path::new("/work/api")), "api");
        assert_eq!(repo_name(Path::new("/")), "repo");
    }
}
