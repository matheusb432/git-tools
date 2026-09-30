//! Discovers and inspects every repository under one traversal root.

use std::path::PathBuf;

use gtl_models::{
    failure::{Classification, Classified, RepositoryFailure},
    repository::{status::StatusResult, traversal::RepositoryTraversalScope},
};

use crate::{
    ports::GitClient,
    repositories::{find_repository_roots, get_repository_statuses},
};

/// Requests repository statuses under a traversal root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetRecursiveRepositoryStatuses {
    pub root: PathBuf,
    pub scope: RepositoryTraversalScope,
}

/// Reports a failure to discover repositories for recursive status inspection.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GetRecursiveRepositoryStatusesError {
    #[error(transparent)]
    Discover(#[from] find_repository_roots::FindRepositoryRootsError),
    #[error("no git repositories found under {}", root.display())]
    NoRepositories { root: PathBuf },
}

impl Classified for GetRecursiveRepositoryStatusesError {
    fn classify(&self) -> Classification {
        match self {
            Self::Discover(error) => error.classify(),
            Self::NoRepositories { root } => Classification::Public(
                RepositoryFailure::NoRepositories { root: root.clone() }.into(),
            ),
        }
    }
}

/// Discovers canonical repository roots and classifies each local status.
///
/// # Errors
///
/// Returns [`GetRecursiveRepositoryStatusesError`] when discovery fails or finds no repositories.
#[cqrsy::query]
pub fn execute(
    query: GetRecursiveRepositoryStatuses,
    git: &impl GitClient,
) -> Result<Vec<StatusResult>, GetRecursiveRepositoryStatusesError> {
    let GetRecursiveRepositoryStatuses { root, scope } = query;
    let repos = find_repository_roots::execute(
        find_repository_roots::FindRepositoryRoots {
            root: root.clone(),
            scope,
        },
        git,
    )?;
    if repos.is_empty() {
        return Err(GetRecursiveRepositoryStatusesError::NoRepositories { root });
    }
    Ok(get_repository_statuses::execute_with_known_descendants(
        repos, git,
    ))
}

#[cfg(test)]
mod tests {
    use gtl_models::repository::traversal::RepositoryTraversalScope;

    use super::{GetRecursiveRepositoryStatuses, GetRecursiveRepositoryStatusesError};
    use crate::{
        repositories::get_recursive_repository_statuses,
        utils::{self, ScriptedGitClient},
    };

    #[test]
    fn reports_an_empty_traversal_as_a_domain_failure() {
        let root = tempfile::tempdir().unwrap();

        let error = get_recursive_repository_statuses::execute(
            GetRecursiveRepositoryStatuses {
                root: root.path().to_path_buf(),
                scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
            },
            &ScriptedGitClient::default(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            GetRecursiveRepositoryStatusesError::NoRepositories { root: error_root }
                if error_root == root.path()
        ));
    }

    #[test]
    fn discovers_and_classifies_repositories_in_one_query() {
        let root = tempfile::tempdir().unwrap();
        utils::make_repository(&root.path().join("api"));
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("//fixture.invalid/repositories/repos/api\n"),
            ScriptedGitClient::applied("HEAD\n"),
            ScriptedGitClient::applied(""),
        ]);

        let results = get_recursive_repository_statuses::execute(
            GetRecursiveRepositoryStatuses {
                root: root.path().to_path_buf(),
                scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
            },
            &git,
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name().as_ref(), "api");
    }
}
