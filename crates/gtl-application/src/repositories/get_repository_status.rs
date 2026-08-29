//! Resolves and inspects the repository containing one path.

use std::path::PathBuf;

use gtl_models::repository::{status::StatusResult, traversal::RepositoryTarget};

use crate::{
    ports::GitClient,
    repositories::{get_repository_statuses, resolve_repository_root},
    shared::repository_name,
};

/// Resolves one repository and classifies its local status.
///
/// # Errors
///
/// Returns [`resolve_repository_root::ResolveRepositoryRootError`] when `repo_path` cannot be
/// resolved to a repository.
#[cqrsy::query]
pub fn execute(
    repo_path: PathBuf,
    git: &impl GitClient,
) -> Result<StatusResult, resolve_repository_root::ResolveRepositoryRootError> {
    let root = resolve_repository_root::execute(repo_path, git)?;
    let target = RepositoryTarget {
        label: repository_name::from_root(&root),
        path: root,
    };
    Ok(get_repository_statuses::get_one(target, git))
}

#[cfg(test)]
mod tests {
    use gtl_models::repository::status::{RepositoryStatus, StatusChanges, StatusHead};

    use crate::{
        repositories::{
            get_repository_status, resolve_repository_root::ResolveRepositoryRootError,
        },
        utils::ScriptedGitClient,
    };

    #[test]
    fn resolves_a_nested_path_before_reading_status() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("HEAD\n"),
            ScriptedGitClient::applied(""),
        ]);

        let result = get_repository_status::execute("/repos/api/src".into(), &git).unwrap();

        assert_eq!(result.name().as_ref(), "api");
        assert_eq!(
            result.repository(),
            &RepositoryStatus::Present {
                head: StatusHead::Detached,
                changes: StatusChanges::Clean,
            }
        );
    }

    #[test]
    fn preserves_a_non_repository_as_a_typed_resolution_failure() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("not a repository")]);

        let error = get_repository_status::execute("/tmp/plain".into(), &git).unwrap_err();

        assert!(matches!(
            error,
            ResolveRepositoryRootError::Rejected { repo_path, .. }
                if repo_path == std::path::Path::new("/tmp/plain")
        ));
    }
}
