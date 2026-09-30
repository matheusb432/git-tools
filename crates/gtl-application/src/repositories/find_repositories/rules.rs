use gtl_models::repository::traversal::RepositoryTraversalScope;

// Name-based pruning accepts false positives below the scan root.
const PRUNED_DIRS: &[&str] = &[".git", "target", "node_modules"];

fn is_pruned_dir(name: &str) -> bool {
    PRUNED_DIRS.contains(&name)
}

/// Reads the Git directory a `.git` file names, which may be relative to its checkout.
pub(super) fn gitfile_target(git_file_contents: &str) -> Option<&str> {
    git_file_contents
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:"))
        .map(str::trim)
        .filter(|target| !target.is_empty())
}

/// Reads the submodule paths a `.gitmodules` file registers, relative to its checkout.
pub(super) fn gitmodules_paths(gitmodules_contents: &str) -> impl Iterator<Item = &str> {
    gitmodules_contents.lines().filter_map(|line| {
        let (key, value) = line.split_once('=')?;
        let value = value.trim();
        key.trim().eq_ignore_ascii_case("path").then(|| {
            value
                .strip_prefix('"')
                .and_then(|quoted| quoted.strip_suffix('"'))
                .unwrap_or(value)
        })
    })
}

/// Never prunes the root, which the caller names explicitly even when it is a linked worktree.
pub(super) fn should_skip(
    depth: usize,
    name: &str,
    is_linked_worktree: bool,
    scope: RepositoryTraversalScope,
) -> bool {
    if depth == 0 {
        return false;
    }
    is_pruned_dir(name) || (is_linked_worktree && !scope.includes_linked_worktrees())
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
    fn gitfile_target_reads_only_the_gitdir_line() {
        assert_eq!(
            gitfile_target("gitdir: //fixture.invalid/repositories/repo/.git/worktrees/feature\n"),
            Some("//fixture.invalid/repositories/repo/.git/worktrees/feature")
        );
        assert_eq!(
            gitfile_target("gitdir: ../.git/modules/sub\n"),
            Some("../.git/modules/sub")
        );
        assert_eq!(gitfile_target("gitdir:\n"), None);
        assert_eq!(gitfile_target("ref: refs/heads/main\n"), None);
    }

    #[test]
    fn gitmodules_paths_reads_each_registered_path() {
        let gitmodules = "[submodule \"api\"]\n\tpath = src/api\n\turl = https://example.invalid/api\n\
                          [submodule \"web\"]\n\tPath = \"src/web\"\n";

        assert_eq!(
            gitmodules_paths(gitmodules).collect::<Vec<_>>(),
            ["src/api", "src/web"]
        );
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
    fn should_skip_always_scans_the_root_even_when_it_is_a_linked_worktree() {
        assert!(!should_skip(
            0,
            "feature",
            true,
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
