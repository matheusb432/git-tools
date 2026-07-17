//! Lists repository worktrees as structured values.

use std::path::PathBuf;

use domain::worktrees::Worktree;

use super::porcelain;
use crate::ports::GitRunner;

/// Requests every worktree registered for one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListWorktrees {
    pub repo: PathBuf,
}

/// Reports either structured worktrees or the Git failure that prevented listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeListResult {
    /// Git listed at least one worktree.
    Listed { worktrees: Vec<Worktree> },
    /// Git rejected the query or returned no worktrees.
    Failed { detail: String },
}

/// Reports an unexpected Git transport failure while listing worktrees.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ListWorktreesError {
    /// Git could not be started or its output could not be collected.
    #[error("{0}")]
    Unexpected(#[source] anyhow::Error),
}

/// Loads worktrees without applying presentation choices.
///
/// # Errors
///
/// Returns [`ListWorktreesError`] when Git cannot be executed.
#[cqrsy::handler(query)]
pub fn execute(
    query: ListWorktrees,
    git: &impl GitRunner,
) -> Result<WorktreeListResult, ListWorktreesError> {
    let ListWorktrees { repo } = query;
    let output = git
        .run(&repo, &["worktree", "list", "--porcelain"])
        .map_err(ListWorktreesError::Unexpected)?;
    if output.exit_code != 0 {
        return Ok(WorktreeListResult::Failed {
            detail: format!("git worktree list failed (exit {})", output.exit_code),
        });
    }

    let worktrees = porcelain::parse(&output.stdout);
    if worktrees.is_empty() {
        Ok(WorktreeListResult::Failed {
            detail: "git returned no worktrees".into(),
        })
    } else {
        Ok(WorktreeListResult::Listed { worktrees })
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{ListWorktrees, WorktreeListResult, execute};
    use crate::testing::FakeGitRunner;

    #[test]
    fn list_parses_branch_detached_locked_and_prunable_values() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok(concat!(
            "worktree /repo\nHEAD 123456789abcdef\nbranch refs/heads/main\n\n",
            "worktree /linked\nHEAD abcdef123456789\ndetached\nlocked maintenance\nprunable gone\n\n",
        ))]);

        let result = execute(ListWorktrees { repo: ".".into() }, &git)
            .expect("porcelain output should produce a structured list");

        let WorktreeListResult::Listed { worktrees } = result else {
            panic!("Git output should produce listed worktrees");
        };
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
        assert!(worktrees[1].detached);
        assert_eq!(worktrees[1].locked.as_deref(), Some("maintenance"));
        assert_eq!(worktrees[1].prunable.as_deref(), Some("gone"));
    }

    #[test]
    fn nonzero_exit_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::exit_err("fatal: not a repo", 128)]);

        assert_eq!(
            execute(ListWorktrees { repo: ".".into() }, &git)
                .expect("a Git rejection is a closed list failure"),
            WorktreeListResult::Failed {
                detail: "git worktree list failed (exit 128)".into(),
            }
        );
    }

    #[test]
    fn empty_porcelain_output_remains_the_exact_closed_failure() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::ok("")]);

        assert_eq!(
            execute(ListWorktrees { repo: ".".into() }, &git)
                .expect("empty Git output is a closed list failure"),
            WorktreeListResult::Failed {
                detail: "git returned no worktrees".into(),
            }
        );
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(ListWorktrees { repo: ".".into() }, &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(error.to_string(), "git transport unavailable");
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
