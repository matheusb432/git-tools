//! Application query for finding and labeling every Git repository under a traversal root.

mod rules;

use std::path::{Path, PathBuf};

use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    repository::traversal::{RepositoryTarget, RepositoryTraversalScope},
};
use walkdir::{DirEntry, WalkDir};

/// Requests repository discovery under a traversal root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindRepositories {
    pub root: PathBuf,
    pub scope: RepositoryTraversalScope,
}

/// Reports a failure to traverse the requested root.
#[derive(Debug, thiserror::Error)]
pub enum FindRepositoriesError {
    #[error("failed to walk {}: {source}", root.display())]
    Walk {
        root: PathBuf,
        #[source]
        source: walkdir::Error,
    },
    #[error("failed to resolve repository {}: {source}", path.display())]
    Resolve {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("resolved repository root {} is invalid: {source}", path.display())]
    InvalidRoot {
        path: PathBuf,
        #[source]
        source: gtl_models::paths::RepositoryRootError,
    },
}

/// Walk `root` and label each discovered repository relative to it.
///
/// # Errors
///
/// Returns [`FindRepositoriesError`] when the traversal cannot read an entry.
#[cqrsy::query]
pub fn execute(req: FindRepositories) -> Result<Vec<RepositoryTarget>, FindRepositoriesError> {
    let FindRepositories { root, scope } = req;
    let mut repos = Vec::new();
    let walk = WalkDir::new(&root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| !is_skipped(entry, scope));
    for entry in walk {
        let entry = entry.map_err(|source| FindRepositoriesError::Walk {
            root: root.clone(),
            source,
        })?;
        if !entry.file_type().is_dir() || !entry.path().join(".git").exists() {
            continue;
        }
        let path = entry.into_path();
        let resolved =
            std::fs::canonicalize(&path).map_err(|source| FindRepositoriesError::Resolve {
                path: path.clone(),
                source,
            })?;
        let repository_root = RepositoryRoot::try_new(resolved.clone()).map_err(|source| {
            FindRepositoriesError::InvalidRoot {
                path: resolved,
                source,
            }
        })?;
        repos.push(repository_root);
    }
    repos.sort();
    Ok(repos
        .into_iter()
        .map(|repo| RepositoryTarget {
            label: repo_label(&root, &repo),
            path: repo,
        })
        .collect())
}

fn is_skipped(entry: &DirEntry, scope: RepositoryTraversalScope) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_str().unwrap_or_default();
    let is_worktree = is_linked_worktree(entry.path());
    rules::should_skip(entry.depth(), name, is_worktree, scope)
}

fn is_linked_worktree(directory: &Path) -> bool {
    let git_path = directory.join(".git");
    if !git_path.is_file() {
        return false;
    }
    std::fs::read_to_string(&git_path).is_ok_and(|content| rules::is_worktree_marker(&content))
}

fn repo_label(root: &Path, repo_path: &RepositoryRoot) -> ProjectName {
    let relative = repo_path.strip_prefix(root).unwrap_or(repo_path.as_ref());
    let label = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if label.is_empty() {
        repo_path.project_name()
    } else {
        ProjectName::try_new(label).unwrap_or_else(|_| repo_path.project_name())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{repositories::find_repositories, utils};

    #[test]
    fn labels_repository_paths_relative_to_the_root() {
        assert_eq!(
            repo_label(
                Path::new("/work"),
                &RepositoryRoot::try_new("/work/api".into()).unwrap()
            )
            .as_str(),
            "api"
        );
        assert_eq!(
            repo_label(
                Path::new("/work"),
                &RepositoryRoot::try_new("/work/libs/inner".into()).unwrap()
            )
            .as_str(),
            "libs/inner"
        );
        assert_eq!(
            repo_label(
                Path::new("/work"),
                &RepositoryRoot::try_new("/work".into()).unwrap()
            )
            .as_str(),
            "work"
        );
    }

    #[test]
    fn skips_linked_worktrees_by_default() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        utils::make_repository(&root.join("api"));
        utils::make_linked_worktree(
            &root.join("api/.worktrees/feature"),
            "/abs/api/.git/worktrees/feature",
        );

        let repositories = find_repositories::execute(FindRepositories {
            root: root.to_path_buf(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].path.as_ref(), root.join("api"));
    }

    #[test]
    fn includes_linked_worktrees_when_requested() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        utils::make_repository(&root.join("api"));
        let worktree = root.join("api/.worktrees/feature");
        utils::make_linked_worktree(&worktree, "/abs/api/.git/worktrees/feature");

        let repositories = find_repositories::execute(FindRepositories {
            root: root.to_path_buf(),
            scope: RepositoryTraversalScope::IncludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(
            repositories
                .iter()
                .map(|repository| repository.path.as_ref())
                .collect::<Vec<_>>(),
            [root.join("api").as_path(), worktree.as_path()]
        );
    }

    #[test]
    fn prunes_build_and_dependency_directories() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        utils::make_repository(&root.join("app"));
        utils::make_repository(&root.join("app/libs/inner"));
        utils::make_repository(&root.join("app/target/some/dependency"));
        utils::make_repository(&root.join("app/node_modules/package"));

        let repositories = find_repositories::execute(FindRepositories {
            root: root.to_path_buf(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(
            repositories
                .iter()
                .map(|repository| repository.path.as_ref())
                .collect::<Vec<_>>(),
            [
                root.join("app").as_path(),
                root.join("app/libs/inner").as_path()
            ]
        );
    }

    #[test]
    fn keeps_submodule_git_pointers() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        utils::make_repository(&root.join("api"));
        utils::make_linked_worktree(&root.join("submodule"), "/repo/.git/modules/submodule");

        let repositories = find_repositories::execute(FindRepositories {
            root: root.to_path_buf(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(
            repositories
                .iter()
                .map(|repository| repository.path.as_ref())
                .collect::<Vec<_>>(),
            [root.join("api").as_path(), root.join("submodule").as_path()]
        );
    }

    #[test]
    fn missing_root_reports_the_failed_traversal() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("missing");

        let error = find_repositories::execute(FindRepositories {
            root: root.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap_err();

        let failed_root = match error {
            FindRepositoriesError::Walk { root, .. } => Some(root),
            FindRepositoriesError::Resolve { .. } | FindRepositoriesError::InvalidRoot { .. } => {
                None
            }
        }
        .unwrap();
        assert_eq!(failed_root, root);
    }
}
