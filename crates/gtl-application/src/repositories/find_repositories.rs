//! Application query for finding and labeling every Git repository under a traversal root.

mod rules;

use std::path::{Path, PathBuf};

use gtl_models::{
    failure::{Classification, Classified, ErrorClass, ExternalDiagnostic, RepositoryFailure},
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

impl Classified for FindRepositoriesError {
    fn classify(&self) -> Classification {
        let (path, diagnostic) = match self {
            Self::Walk { root, source } => (root, source.to_string()),
            Self::Resolve { path, source } => (path, source.to_string()),
            Self::InvalidRoot { .. } => return Classification::Private(ErrorClass::Internal),
        };
        Classification::Public(
            RepositoryFailure::SearchFailed {
                path: path.clone(),
                diagnostic: ExternalDiagnostic::new(&diagnostic),
            }
            .into(),
        )
    }
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
    let label_root =
        std::fs::canonicalize(&root).map_err(|source| FindRepositoriesError::Resolve {
            path: root.clone(),
            source,
        })?;
    let label_root = RepositoryRoot::try_new(label_root.clone()).map_err(|source| {
        FindRepositoriesError::InvalidRoot {
            path: label_root,
            source,
        }
    })?;
    repos.sort();
    Ok(repos
        .into_iter()
        .map(|repo| RepositoryTarget {
            label: repo_label(&label_root, &repo),
            path: repo,
        })
        .collect())
}

fn is_skipped(entry: &DirEntry, scope: RepositoryTraversalScope) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_str().unwrap_or_default();
    let is_worktree = is_linked_worktree(entry.path()) && !is_submodule_checkout(entry.path());
    rules::should_skip(entry.depth(), name, is_worktree, scope)
}

// Git gives only a linked worktree's administrative directory a `commondir` file.
fn is_linked_worktree(directory: &Path) -> bool {
    let git_path = directory.join(".git");
    if !git_path.is_file() {
        return false;
    }
    std::fs::read_to_string(&git_path).is_ok_and(|content| {
        rules::gitfile_target(&content)
            .is_some_and(|target| directory.join(target).join("commondir").is_file())
    })
}

// A submodule checkout can itself be a linked worktree of another checkout's submodule repository.
fn is_submodule_checkout(directory: &Path) -> bool {
    let Some(superproject) = directory
        .ancestors()
        .skip(1)
        .find(|ancestor| ancestor.join(".git").exists())
    else {
        return false;
    };
    let Ok(gitmodules) = std::fs::read_to_string(superproject.join(".gitmodules")) else {
        return false;
    };
    directory
        .strip_prefix(superproject)
        .is_ok_and(|submodule_path| {
            rules::gitmodules_paths(&gitmodules).any(|path| Path::new(path) == submodule_path)
        })
}

/// Checks whether a scan rooted at `directory` reports the directory itself.
#[must_use]
pub fn is_discoverable_repository(directory: &Path) -> bool {
    directory.is_dir() && directory.join(".git").exists()
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
                Path::new("//fixture.invalid/repositories/work"),
                &RepositoryRoot::try_new("//fixture.invalid/repositories/work/api".into()).unwrap()
            )
            .as_str(),
            "api"
        );
        assert_eq!(
            repo_label(
                Path::new("//fixture.invalid/repositories/work"),
                &RepositoryRoot::try_new("//fixture.invalid/repositories/work/libs/inner".into())
                    .unwrap()
            )
            .as_str(),
            "libs/inner"
        );
        assert_eq!(
            repo_label(
                Path::new("//fixture.invalid/repositories/work"),
                &RepositoryRoot::try_new("//fixture.invalid/repositories/work".into()).unwrap()
            )
            .as_str(),
            "work"
        );
    }

    #[cfg(unix)]
    #[test]
    fn labels_repositories_relative_to_a_symlinked_root() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("actual");
        utils::make_repository(&root.join("api"));
        let alias = temporary.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();

        let repositories = find_repositories::execute(FindRepositories {
            root: alias,
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].label.as_str(), "api");
        assert_eq!(
            repositories[0].path.as_ref(),
            root.join("api").canonicalize().unwrap()
        );
    }

    #[test]
    fn skips_linked_worktrees_by_default() {
        let temporary = tempfile::tempdir().unwrap();
        let root = RepositoryRoot::try_new(temporary.path().canonicalize().unwrap())
            .unwrap()
            .as_ref()
            .to_path_buf();
        utils::make_repository(&root.join("api"));
        utils::make_linked_worktree(
            &root.join("api/.worktrees/feature"),
            &root.join("api/.git/worktrees/feature"),
        );

        let repositories = find_repositories::execute(FindRepositories {
            root: root.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].path.as_ref(), root.join("api"));
    }

    #[test]
    fn scans_a_linked_worktree_root_and_its_submodules_but_not_nested_worktrees() {
        let temporary = tempfile::tempdir().unwrap();
        let root = RepositoryRoot::try_new(temporary.path().canonicalize().unwrap())
            .unwrap()
            .as_ref()
            .to_path_buf();
        let administrative_directory = root.join("api/.git/worktrees/feature");
        let worktree = root.join("api/.worktrees/feature");
        utils::make_repository(&root.join("api"));
        utils::make_linked_worktree(&worktree, &administrative_directory);
        utils::make_submodule(
            &worktree.join("lib/submodule"),
            &administrative_directory.join("modules/lib/submodule"),
        );
        utils::make_linked_worktree(
            &worktree.join(".worktrees/nested"),
            &root.join("api/.git/worktrees/nested"),
        );

        let repositories = find_repositories::execute(FindRepositories {
            root: worktree.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(
            repositories
                .iter()
                .map(|repository| repository.path.as_ref())
                .collect::<Vec<_>>(),
            [worktree.as_path(), worktree.join("lib/submodule").as_path()]
        );
        assert!(is_discoverable_repository(&worktree));
    }

    #[test]
    fn keeps_submodule_checkouts_that_are_linked_worktrees() {
        let temporary = tempfile::tempdir().unwrap();
        let root = RepositoryRoot::try_new(temporary.path().canonicalize().unwrap())
            .unwrap()
            .as_ref()
            .to_path_buf();
        let worktree = root.join("api/.worktrees/feature");
        utils::make_repository(&root.join("api"));
        utils::make_linked_worktree(&worktree, &root.join("api/.git/worktrees/feature"));
        std::fs::write(
            worktree.join(".gitmodules"),
            "[submodule \"submodule\"]\n\tpath = lib/submodule\n",
        )
        .unwrap();
        utils::make_linked_worktree(
            &worktree.join("lib/submodule"),
            &root.join("api/.git/modules/submodule/worktrees/submodule"),
        );

        let repositories = find_repositories::execute(FindRepositories {
            root: worktree.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(
            repositories
                .iter()
                .map(|repository| repository.path.as_ref())
                .collect::<Vec<_>>(),
            [worktree.as_path(), worktree.join("lib/submodule").as_path()]
        );
    }

    #[test]
    fn includes_linked_worktrees_when_requested() {
        let temporary = tempfile::tempdir().unwrap();
        let root = RepositoryRoot::try_new(temporary.path().canonicalize().unwrap())
            .unwrap()
            .as_ref()
            .to_path_buf();
        utils::make_repository(&root.join("api"));
        let worktree = root.join("api/.worktrees/feature");
        utils::make_linked_worktree(&worktree, &root.join("api/.git/worktrees/feature"));

        let repositories = find_repositories::execute(FindRepositories {
            root: root.clone(),
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
        let root = RepositoryRoot::try_new(temporary.path().canonicalize().unwrap())
            .unwrap()
            .as_ref()
            .to_path_buf();
        utils::make_repository(&root.join("app"));
        utils::make_repository(&root.join("app/libs/inner"));
        utils::make_repository(&root.join("app/target/some/dependency"));
        utils::make_repository(&root.join("app/node_modules/package"));

        let repositories = find_repositories::execute(FindRepositories {
            root: root.clone(),
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
        let root = RepositoryRoot::try_new(temporary.path().canonicalize().unwrap())
            .unwrap()
            .as_ref()
            .to_path_buf();
        utils::make_repository(&root.join("api"));
        utils::make_submodule(
            &root.join("api/lib/submodule"),
            &root.join("api/.git/modules/lib/submodule"),
        );

        let repositories = find_repositories::execute(FindRepositories {
            root: root.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })
        .unwrap();

        assert_eq!(
            repositories
                .iter()
                .map(|repository| repository.path.as_ref())
                .collect::<Vec<_>>(),
            [
                root.join("api").as_path(),
                root.join("api/lib/submodule").as_path()
            ]
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
