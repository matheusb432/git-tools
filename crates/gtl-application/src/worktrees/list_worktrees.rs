//! Lists repository worktrees as structured values.

use gtl_models::{paths::RepositoryRoot, worktrees::Worktree};

use crate::ports::{GitClient, GitEffect};

/// Requests every worktree registered for one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListWorktrees {
    pub repo_path: RepositoryRoot,
}

/// Reports either structured worktrees or the Git failure that prevented listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListWorktreesOk {
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
#[cqrsy::query]
pub fn execute(
    query: ListWorktrees,
    git: &impl GitClient,
) -> Result<ListWorktreesOk, ListWorktreesError> {
    let ListWorktrees { repo_path } = query;
    let worktrees = git
        .worktrees(&repo_path)
        .map_err(ListWorktreesError::Unexpected)?;
    let worktrees = match worktrees {
        GitEffect::Applied(worktrees) => worktrees,
        GitEffect::Rejected(detail) => return Ok(ListWorktreesOk::Failed { detail }),
    };
    if worktrees.is_empty() {
        Ok(ListWorktreesOk::Failed {
            detail: "git returned no worktrees".into(),
        })
    } else {
        Ok(ListWorktreesOk::Listed { worktrees })
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use gtl_models::worktrees::{WorktreeCheckout, WorktreeKind};

    use super::{ListWorktrees, ListWorktreesOk};
    use crate::{utils::ScriptedGitClient, worktrees::list_worktrees};

    #[test]
    fn list_parses_branch_detached_locked_and_prunable_values() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied(concat!(
            "worktree /repo\nHEAD 123456789abcdef\nbranch refs/heads/main\n\n",
            "worktree /linked\nHEAD abcdef123456789\ndetached\nlocked maintenance\nprunable gone\n\n",
        ))]);

        let result = list_worktrees::execute(
            ListWorktrees {
                repo_path: crate::utils::repository_root("/repo"),
            },
            &git,
        )
        .expect("porcelain output should produce a structured list");

        let ListWorktreesOk::Listed { worktrees } = result else {
            panic!("Git output should produce listed worktrees");
        };
        assert_eq!(worktrees.len(), 2);
        assert!(matches!(
            worktrees[0].kind(),
            WorktreeKind::Checkout(WorktreeCheckout::Branch(branch)) if branch.as_ref() == "main"
        ));
        assert_eq!(
            worktrees[1].kind(),
            &WorktreeKind::Checkout(WorktreeCheckout::Detached)
        );
        assert_eq!(worktrees[1].locked(), Some("maintenance"));
        assert_eq!(worktrees[1].prunable(), Some("gone"));
    }

    #[test]
    fn rejected_listing_preserves_the_adapter_diagnostic() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("fatal: not a repo")]);

        assert_eq!(
            list_worktrees::execute(
                ListWorktrees {
                    repo_path: crate::utils::repository_root("/repo"),
                },
                &git,
            )
            .expect("a Git rejection is a closed list failure"),
            ListWorktreesOk::Failed {
                detail: "fatal: not a repo".into(),
            }
        );
    }

    #[test]
    fn empty_porcelain_output_remains_the_exact_closed_failure() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);

        assert_eq!(
            list_worktrees::execute(
                ListWorktrees {
                    repo_path: crate::utils::repository_root("/repo"),
                },
                &git,
            )
            .expect("empty Git output is a closed list failure"),
            ListWorktreesOk::Failed {
                detail: "git returned no worktrees".into(),
            }
        );
    }

    #[test]
    fn transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = list_worktrees::execute(
            ListWorktrees {
                repo_path: crate::utils::repository_root("/repo"),
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
