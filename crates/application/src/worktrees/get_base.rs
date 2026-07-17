//! Gets the primary path from a repository's registered worktrees.

use std::path::PathBuf;

use super::porcelain;
use crate::ports::GitRunner;

/// Requests the primary worktree path for one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetWorktreeBase {
    pub repo: PathBuf,
}

/// Reports either the primary worktree path or the Git failure that prevented discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeBaseResult {
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
#[cqrsy::handler(query)]
pub fn execute(
    query: GetWorktreeBase,
    git: &impl GitRunner,
) -> Result<WorktreeBaseResult, GetWorktreeBaseError> {
    let GetWorktreeBase { repo } = query;
    let output = git
        .run(&repo, &["worktree", "list", "--porcelain"])
        .map_err(GetWorktreeBaseError::Unexpected)?;
    if output.exit_code != 0 {
        return Ok(WorktreeBaseResult::Failed {
            detail: format!("git worktree list failed (exit {})", output.exit_code),
        });
    }

    match porcelain::parse(&output.stdout).into_iter().next() {
        Some(worktree) => Ok(WorktreeBaseResult::Found {
            path: worktree.path,
        }),
        None => Ok(WorktreeBaseResult::Failed {
            detail: "git returned no worktrees".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{GetWorktreeBase, WorktreeBaseResult, execute};
    use crate::testing::FakeGitRunner;

    #[test]
    fn base_returns_the_first_porcelain_worktree_path() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok(concat!(
            "worktree /repo\nHEAD 123456789abcdef\nbranch refs/heads/main\n\n",
            "worktree /linked\nHEAD abcdef123456789\nbranch refs/heads/feature\n\n",
        ))]);

        assert_eq!(
            execute(GetWorktreeBase { repo: ".".into() }, &git)
                .expect("porcelain output should produce a base path"),
            WorktreeBaseResult::Found {
                path: "/repo".into(),
            }
        );
    }

    #[test]
    fn nonzero_exit_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::exit_err("fatal: not a repo", 128)]);

        assert_eq!(
            execute(GetWorktreeBase { repo: ".".into() }, &git)
                .expect("a Git rejection is a closed base failure"),
            WorktreeBaseResult::Failed {
                detail: "git worktree list failed (exit 128)".into(),
            }
        );
    }

    #[test]
    fn empty_porcelain_output_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok("")]);

        assert_eq!(
            execute(GetWorktreeBase { repo: ".".into() }, &git)
                .expect("empty Git output is a closed base failure"),
            WorktreeBaseResult::Failed {
                detail: "git returned no worktrees".into(),
            }
        );
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(GetWorktreeBase { repo: ".".into() }, &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
