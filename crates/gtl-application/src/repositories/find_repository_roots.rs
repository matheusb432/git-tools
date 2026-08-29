//! Finds every Git repository under a root (via [`find_repositories`]) and resolves
//! each to its canonical top-level path through
//! the [`GitClient`](crate::ports::GitClient) port for recursive server requests.

use std::path::PathBuf;

use gtl_models::{
    paths::RepositoryRoot,
    repository::traversal::{RepositoryTarget, RepositoryTraversalScope},
};

use crate::{ports::GitClient, repositories::find_repositories};

/// Discover every git repo under `root`, resolved to its canonical top level and
/// labeled relative to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindRepositoryRoots {
    pub root: PathBuf,
    pub scope: RepositoryTraversalScope,
}

/// Everything that can go wrong discovering and resolving repo tops.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FindRepositoryRootsError {
    /// Reports that repositories could not be discovered under the requested root.
    #[error(transparent)]
    Discover(#[from] find_repositories::FindRepositoriesError),
    /// Reports that a discovered repository's canonical top level could not be resolved.
    #[error("{source}")]
    Resolve {
        repo_root: RepositoryRoot,
        #[source]
        source: anyhow::Error,
    },
    /// Reports that Git rejected a discovered repository during top-level resolution.
    #[error("not a git repo: {repo_root}")]
    Rejected { repo_root: RepositoryRoot },
}

/// Walk `root` through repository traversal and resolve each repo's `path` to its
/// `git rev-parse --show-toplevel`. Labels stay relative to `root`.
///
/// # Errors
///
/// Returns [`FindRepositoryRootsError::Discover`] when repository discovery fails, or
/// [`FindRepositoryRootsError::Resolve`] when a discovered repository's canonical top
/// level cannot be resolved.
#[cqrsy::query]
pub fn execute(
    req: FindRepositoryRoots,
    git: &impl GitClient,
) -> Result<Vec<RepositoryTarget>, FindRepositoryRootsError> {
    let FindRepositoryRoots { root, scope } = req;
    let discovered =
        find_repositories::execute(find_repositories::FindRepositories { root, scope })?;
    discovered
        .into_iter()
        .map(|repo| {
            let RepositoryTarget { path, label } = repo;
            let resolved = git.discover_top(path.as_ref()).map_err(|source| {
                FindRepositoryRootsError::Resolve {
                    repo_root: path.clone(),
                    source,
                }
            })?;
            Ok(RepositoryTarget {
                path: resolved.ok_or(FindRepositoryRootsError::Rejected { repo_root: path })?,
                label,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        repositories::find_repository_roots,
        utils::{self, ScriptedGitClient},
    };

    #[test]
    fn resolves_each_discovered_repo_to_its_top_level() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        utils::make_repository(&root.join("api"));
        utils::make_repository(&root.join("libs/inner"));
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/real/api\n"),
            ScriptedGitClient::applied("/real/libs/inner\n"),
        ]);

        let tops = find_repository_roots::execute(
            FindRepositoryRoots {
                root: root.to_path_buf(),
                scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
            },
            &runner,
        )
        .unwrap();

        assert_eq!(
            tops,
            vec![
                RepositoryTarget {
                    path: utils::repository_root("/real/api"),
                    label: utils::project_name("api"),
                },
                RepositoryTarget {
                    path: utils::repository_root("/real/libs/inner"),
                    label: utils::project_name("libs/inner"),
                },
            ]
        );
    }

    #[test]
    fn a_failed_top_level_resolution_keeps_the_legacy_error_shape() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("api");
        utils::make_repository(&repository);
        let runner = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("fatal: not a repo")]);

        let error = find_repository_roots::execute(
            FindRepositoryRoots {
                root: temporary.path().to_path_buf(),
                scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
            },
            &runner,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("not a git repo: {}", repository.display())
        );
    }
}
