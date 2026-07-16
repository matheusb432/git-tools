//! Resolves one path to its canonical Git repository top level.

use std::path::PathBuf;

use crate::ports::GitRunner;

/// Requests the repository top level containing `repo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveRepoTop {
    pub repo: PathBuf,
}

/// Reports a failure to resolve a repository top level.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveRepoTopError {
    /// Git transport failed before returning a command result.
    #[error("{source}\nnot a git repo: {}", repo.display())]
    Transport {
        repo: PathBuf,
        #[source]
        source: anyhow::Error,
    },
    /// Git rejected the path or returned no top-level path.
    #[error("{detail}")]
    Rejected { repo: PathBuf, detail: String },
}

/// Resolves a path to the top level of its containing Git repository.
///
/// # Errors
///
/// Returns [`ResolveRepoTopError`] when Git transport fails or the path is not
/// inside a repository.
#[cqrsy::handler(query)]
pub fn execute(
    query: ResolveRepoTop,
    git: &impl GitRunner,
) -> Result<PathBuf, ResolveRepoTopError> {
    let output = git
        .run(&query.repo, &["rev-parse", "--show-toplevel"])
        .map_err(|source| ResolveRepoTopError::Transport {
            repo: query.repo.clone(),
            source,
        })?;
    let top = output.stdout.trim();
    if output.success() && !top.is_empty() {
        return Ok(PathBuf::from(top));
    }

    let diagnostic = output.diagnostic();
    let detail = if diagnostic.is_empty() {
        format!(
            "git exited with exit status: {}\nnot a git repo: {}",
            output.exit_code,
            query.repo.display()
        )
    } else {
        format!("{diagnostic}\nnot a git repo: {}", query.repo.display())
    };
    Err(ResolveRepoTopError::Rejected {
        repo: query.repo,
        detail,
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{ResolveRepoTop, ResolveRepoTopError, execute};
    use crate::testing::FakeGitRunner;

    #[test]
    fn resolves_a_nested_path_to_its_repository_top() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok("/repos/api\n")]);

        let top = execute(
            ResolveRepoTop {
                repo: "/repos/api/src".into(),
            },
            &git,
        )
        .expect("top resolves");

        assert_eq!(top, std::path::PathBuf::from("/repos/api"));
    }

    #[test]
    fn transport_failure_remains_the_error_source() {
        let git = FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git unavailable"))]);

        let error = execute(
            ResolveRepoTop {
                repo: "/repos/api".into(),
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
    fn silent_git_rejection_keeps_the_exit_status_in_its_detail() {
        let git = FakeGitRunner::new(vec![crate::ports::GitOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 128,
        }]);

        let error = execute(
            ResolveRepoTop {
                repo: "/repos/api".into(),
            },
            &git,
        )
        .expect_err("silent rejection fails");

        assert_eq!(
            error.to_string(),
            "git exited with exit status: 128\nnot a git repo: /repos/api"
        );
    }
}
