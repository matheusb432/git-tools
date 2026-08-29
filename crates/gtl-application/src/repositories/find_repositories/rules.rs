use gtl_models::repository::traversal::RepositoryTraversalScope;

// Name-based pruning accepts false positives below the scan root.
const PRUNED_DIRS: &[&str] = &[".git", "target", "node_modules"];

fn is_pruned_dir(name: &str) -> bool {
    PRUNED_DIRS.contains(&name)
}

/// Distinguishes linked-worktree gitfiles from submodule gitfiles.
pub(super) fn is_worktree_marker(git_file_contents: &str) -> bool {
    git_file_contents
        .lines()
        .filter_map(|line| line.strip_prefix("gitdir:"))
        .any(|target| target.trim().replace('\\', "/").contains("/worktrees/"))
}

pub(super) fn should_skip(
    depth: usize,
    name: &str,
    is_linked_worktree: bool,
    scope: RepositoryTraversalScope,
) -> bool {
    if !scope.includes_linked_worktrees() && is_linked_worktree {
        return true;
    }
    depth > 0 && is_pruned_dir(name)
}

#[cfg(test)]
mod tests {
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
        assert!(!is_worktree_marker("gitdir: /repo/.git/modules/sub\n"));
        assert!(is_worktree_marker("gitdir: C:\\repo\\.git\\worktrees\\f\n"));
        assert!(!is_worktree_marker("ref: refs/heads/main\n"));
    }

    #[test]
    fn should_skip_always_scans_the_root_even_when_named_like_a_pruned_dir() {
        assert!(!should_skip(
            0,
            "target",
            false,
            RepositoryTraversalScope::ExcludeLinkedWorktrees
        ));
    }

    #[test]
    fn should_skip_prunes_build_dirs_below_the_root() {
        assert!(should_skip(
            1,
            "target",
            false,
            RepositoryTraversalScope::ExcludeLinkedWorktrees
        ));
        assert!(!should_skip(
            1,
            "src",
            false,
            RepositoryTraversalScope::ExcludeLinkedWorktrees
        ));
    }

    #[test]
    fn should_skip_honors_the_worktree_flag() {
        assert!(should_skip(
            2,
            "feature",
            true,
            RepositoryTraversalScope::ExcludeLinkedWorktrees
        ));
        assert!(!should_skip(
            2,
            "feature",
            true,
            RepositoryTraversalScope::IncludeLinkedWorktrees
        ));
    }
}
