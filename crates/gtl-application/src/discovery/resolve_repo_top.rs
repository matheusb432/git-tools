//! Resolves one path to its canonical Git repository top level.

use std::path::PathBuf;

use crate::ports::GitClient;

/// Requests the repository top level containing `repo_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveRepoTop {
    pub repo_path: PathBuf,
}

pub type ResolveRepoTopOk = PathBuf;

/// Reports a failure to resolve a repository top level.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveRepoTopError {
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

/// Resolves a path to the top level of its containing Git repository.
///
/// # Errors
///
/// Returns [`ResolveRepoTopError`] when Git transport fails or the path is not
/// inside a repository.
#[cqrsy::query]
pub fn execute(
    query: ResolveRepoTop,
    git: &impl GitClient,
) -> Result<ResolveRepoTopOk, ResolveRepoTopError> {
    let top =
        git.discover_top(&query.repo_path)
            .map_err(|source| ResolveRepoTopError::Transport {
                repo_path: query.repo_path.clone(),
                source,
            })?;
    if let Some(top) = top {
        return Ok(top);
    }

    Err(ResolveRepoTopError::Rejected {
        detail: format!("not a git repo: {}", query.repo_path.display()),
        repo_path: query.repo_path,
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{ResolveRepoTop, ResolveRepoTopError, execute};
    use crate::testing::ScriptedGitClient;

    #[test]
    fn resolves_a_nested_path_to_its_repository_top() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("/repos/api\n")]);

        let top = execute(
            ResolveRepoTop {
                repo_path: "/repos/api/src".into(),
            },
            &git,
        )
        .expect("top resolves");

        assert_eq!(top, std::path::PathBuf::from("/repos/api"));
    }

    #[test]
    fn transport_failure_remains_the_error_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!("git unavailable"))]);

        let error = execute(
            ResolveRepoTop {
                repo_path: "/repos/api".into(),
            },
            &git,
        )
        .expect_err("transport fails");

        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git unavailable".into())
        );
        assert!(matches!(error, ResolveRepoTopError::Transport { .. }));
    }

    #[test]
    fn silent_git_rejection_reports_the_semantic_failure() {
        let git =
            ScriptedGitClient::new(vec![crate::testing::GitResponse::Rejected(String::new())]);

        let error = execute(
            ResolveRepoTop {
                repo_path: "/repos/api".into(),
            },
            &git,
        )
        .expect_err("silent rejection fails");

        assert_eq!(error.to_string(), "not a git repo: /repos/api");
    }
}
