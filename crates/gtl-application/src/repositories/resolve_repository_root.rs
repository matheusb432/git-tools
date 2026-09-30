//! Resolves one path to its canonical Git repository top level.

use std::path::PathBuf;

use gtl_models::{
    failure::{Classification, Classified, ErrorClass, RepositoryFailure},
    paths::RepositoryRoot,
};

use crate::ports::GitClient;

/// Reports a failure to resolve a repository top level.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveRepositoryRootError {
    /// Git transport failed before returning a command result.
    #[error("{source}\nnot a git repo: {}", repo_path.display())]
    Transport {
        repo_path: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    /// Git rejected the path or returned no top-level path.
    #[error("{detail}")]
    Rejected { repo_path: PathBuf, detail: String },
}

impl Classified for ResolveRepositoryRootError {
    fn classify(&self) -> Classification {
        match self {
            Self::Transport { .. } => Classification::Private(ErrorClass::Internal),
            Self::Rejected { repo_path, .. } => Classification::Public(
                RepositoryFailure::NotARepository {
                    path: repo_path.clone(),
                }
                .into(),
            ),
        }
    }
}

/// Resolves a path to the top level of its containing Git repository.
///
/// # Errors
///
/// Returns [`ResolveRepositoryRootError`] when Git transport fails or the path is not
/// inside a repository.
#[cqrsy::query]
pub fn execute(
    repo_path: PathBuf,
    git: &impl GitClient,
) -> Result<RepositoryRoot, ResolveRepositoryRootError> {
    let top =
        git.discover_top(&repo_path)
            .map_err(|source| ResolveRepositoryRootError::Transport {
                repo_path: repo_path.clone(),
                source,
            })?;
    if let Some(top) = top {
        return Ok(top);
    }

    Err(ResolveRepositoryRootError::Rejected {
        detail: format!("not a git repo: {}", repo_path.display()),
        repo_path,
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::ResolveRepositoryRootError;
    use crate::{repositories::resolve_repository_root, utils::ScriptedGitClient};

    #[test]
    fn resolves_a_nested_path_to_its_repository_top() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied(
            "//fixture.invalid/repositories/repos/api\n",
        )]);

        let top = resolve_repository_root::execute(
            "//fixture.invalid/repositories/repos/api/src".into(),
            &git,
        )
        .unwrap();

        assert_eq!(
            top.as_ref(),
            std::path::Path::new("//fixture.invalid/repositories/repos/api")
        );
    }

    #[test]
    fn transport_failure_remains_the_error_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!("git unavailable"))]);

        let error = resolve_repository_root::execute(
            "//fixture.invalid/repositories/repos/api".into(),
            &git,
        )
        .unwrap_err();

        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git unavailable".into())
        );
        assert!(matches!(
            error,
            ResolveRepositoryRootError::Transport { .. }
        ));
    }

    #[test]
    fn silent_git_rejection_reports_the_semantic_failure() {
        let git = ScriptedGitClient::new(vec![crate::utils::GitResponse::Rejected(String::new())]);

        let error = resolve_repository_root::execute(
            "//fixture.invalid/repositories/repos/api".into(),
            &git,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "not a git repo: //fixture.invalid/repositories/repos/api"
        );
    }
}
