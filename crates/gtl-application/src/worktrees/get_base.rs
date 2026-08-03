//! Gets the primary path from a repository's registered worktrees.

use std::path::PathBuf;

use crate::ports::{GitClient, GitEffect};

/// Requests the primary worktree path for one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetWorktreeBase {
    pub repo_path: PathBuf,
}

/// Reports either the primary worktree path or the Git failure that prevented discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetWorktreeBaseOk {
    /// Git listed a primary worktree path.
    Found { path: String },
    /// Git rejected the query or returned no worktrees.
    Failed { detail: String },
}

/// Reports an unexpected Git transport failure while getting the primary worktree.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GetWorktreeBaseError {
    /// Git could not be started or its output could not be collected.
    #[error("{0}")]
    Unexpected(#[source] anyhow::Error),
}

/// Gets the first path from Git's ordered worktree list.
///
/// # Errors
///
/// Returns [`GetWorktreeBaseError`] when Git cannot be executed.
#[cqrsy::query]
pub fn execute(
    query: GetWorktreeBase,
    git: &impl GitClient,
) -> Result<GetWorktreeBaseOk, GetWorktreeBaseError> {
    let GetWorktreeBase { repo_path } = query;
    let worktrees = git
        .worktrees(&repo_path)
        .map_err(GetWorktreeBaseError::Unexpected)?;
    let worktrees = match worktrees {
        GitEffect::Applied(worktrees) => worktrees,
        GitEffect::Rejected(detail) => return Ok(GetWorktreeBaseOk::Failed { detail }),
    };
    match worktrees.into_iter().next() {
        Some(worktree) => Ok(GetWorktreeBaseOk::Found {
            path: worktree.path,
        }),
        None => Ok(GetWorktreeBaseOk::Failed {
            detail: "git returned no worktrees".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{GetWorktreeBase, GetWorktreeBaseOk, execute};
    use crate::testing::ScriptedGitClient;

    #[test]
    fn base_returns_the_first_porcelain_worktree_path() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied(concat!(
            "worktree /repo\nHEAD 123456789abcdef\nbranch refs/heads/main\n\n",
            "worktree /linked\nHEAD abcdef123456789\nbranch refs/heads/feature\n\n",
        ))]);

        assert_eq!(
            execute(
                GetWorktreeBase {
                    repo_path: ".".into(),
                },
                &git,
            )
            .expect("porcelain output should produce a base path"),
            GetWorktreeBaseOk::Found {
                path: "/repo".into(),
            }
        );
    }

    #[test]
    fn rejected_listing_preserves_the_adapter_diagnostic() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("fatal: not a repo")]);

        assert_eq!(
            execute(
                GetWorktreeBase {
                    repo_path: ".".into(),
                },
                &git,
            )
            .expect("a Git rejection is a closed base failure"),
            GetWorktreeBaseOk::Failed {
                detail: "fatal: not a repo".into(),
            }
        );
    }

    #[test]
    fn empty_porcelain_output_remains_the_exact_closed_failure() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

        assert_eq!(
            execute(
                GetWorktreeBase {
                    repo_path: ".".into(),
                },
                &git,
            )
            .expect("empty Git output is a closed base failure"),
            GetWorktreeBaseOk::Failed {
                detail: "git returned no worktrees".into(),
            }
        );
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            GetWorktreeBase {
                repo_path: ".".into(),
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
