//! Applies a confirmed branch-prune plan.

use std::path::PathBuf;

use super::plan_prune::PruneBranch;
use crate::ports::GitClient;

/// Requests deletion of the selected local branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPrune {
    pub top: PathBuf,
    pub branches: Vec<PruneBranch>,
}

/// Identifies one selected branch that Git could not delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneFailure {
    pub name: String,
    pub reason: String,
}

/// Classifies the aggregate result of deleting selected branches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneStatus {
    Ok,
    Partial,
    Fail,
}

/// Reports deleted branches and failures without presentation-specific text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPruneOk {
    pub status: PruneStatus,
    pub deleted: Vec<PruneBranch>,
    pub failed: Vec<PruneFailure>,
}

/// Reports an unexpected Git transport failure while applying branch pruning.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyPruneError {
    /// Git transport failed before every selected branch was attempted.
    #[error("branch prune application failed: {source}")]
    Transport {
        /// Preserves completed deletion and rejection outcomes, when any exist.
        completed_result: Option<Box<ApplyPruneOk>>,
        /// Preserves the original Git transport failure.
        #[source]
        source: anyhow::Error,
    },
}

/// Deletes each selected branch in plan order with Git's force-delete operation.
///
/// # Errors
///
/// Returns [`ApplyPruneError`] when Git cannot be executed before every selected branch is
/// attempted. Completed branch outcomes remain available on the error.
#[cqrsy::command]
pub fn execute(command: ApplyPrune, git: &impl GitClient) -> Result<ApplyPruneOk, ApplyPruneError> {
    let ApplyPrune { top, branches } = command;
    let mut deleted = Vec::new();
    let mut failed = Vec::new();

    for branch in branches {
        match git.delete_branch(&top, &branch.name) {
            Ok(crate::ports::GitEffect::Applied(())) => deleted.push(branch),
            Ok(crate::ports::GitEffect::Rejected(reason)) => failed.push(PruneFailure {
                name: branch.name,
                reason,
            }),
            Err(source) => {
                let completed_result = (!deleted.is_empty() || !failed.is_empty())
                    .then(|| Box::new(classify_result(deleted, failed)));
                return Err(ApplyPruneError::Transport {
                    completed_result,
                    source,
                });
            }
        }
    }

    Ok(classify_result(deleted, failed))
}

fn classify_result(deleted: Vec<PruneBranch>, failed: Vec<PruneFailure>) -> ApplyPruneOk {
    let status = match (deleted.is_empty(), failed.is_empty()) {
        (_, true) => PruneStatus::Ok,
        (true, false) => PruneStatus::Fail,
        (false, false) => PruneStatus::Partial,
    };

    ApplyPruneOk {
        status,
        deleted,
        failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{branches::plan_prune::PruneBranch, testing::ScriptedGitClient};

    #[test]
    fn apply_reports_deleted_and_failed_branches_as_values() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("error: branch 'fix/z' is checked out"),
        ]);
        let deleted = PruneBranch {
            name: "feature/done".into(),
            sha: "aaaaaaa".into(),
        };
        let failed = PruneBranch {
            name: "fix/z".into(),
            sha: "bbbbbbb".into(),
        };

        let result = execute(
            ApplyPrune {
                top: "/repo".into(),
                branches: vec![deleted.clone(), failed],
            },
            &git,
        )
        .expect("Git rejection remains a closed branch failure");

        assert_eq!(
            result,
            ApplyPruneOk {
                status: PruneStatus::Partial,
                deleted: vec![deleted],
                failed: vec![PruneFailure {
                    name: "fix/z".into(),
                    reason: "error: branch 'fix/z' is checked out".into(),
                }],
            }
        );
    }

    #[test]
    fn apply_preserves_plan_order_in_deleted_recovery_values() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
        ]);
        let branches = vec![
            PruneBranch {
                name: "feature/first".into(),
                sha: "aaaaaaa".into(),
            },
            PruneBranch {
                name: "feature/second".into(),
                sha: "bbbbbbb".into(),
            },
        ];

        let result = execute(
            ApplyPrune {
                top: "/repo".into(),
                branches: branches.clone(),
            },
            &git,
        )
        .expect("successful deletions remain a closed result");

        assert_eq!(
            result,
            ApplyPruneOk {
                status: PruneStatus::Ok,
                deleted: branches,
                failed: Vec::new(),
            }
        );
    }

    #[test]
    fn apply_reports_fail_when_every_deletion_fails() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("deletion rejected")]);

        let result = execute(
            ApplyPrune {
                top: "/repo".into(),
                branches: vec![PruneBranch {
                    name: "feature/blocked".into(),
                    sha: "aaaaaaa".into(),
                }],
            },
            &git,
        )
        .expect("Git rejection remains a closed branch failure");

        assert_eq!(result.status, PruneStatus::Fail);
        assert!(result.deleted.is_empty());
        assert_eq!(
            result.failed,
            vec![PruneFailure {
                name: "feature/blocked".into(),
                reason: "deletion rejected".into(),
            }]
        );
    }

    #[test]
    fn apply_transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            ApplyPrune {
                top: "/repo".into(),
                branches: vec![PruneBranch {
                    name: "feature/done".into(),
                    sha: "aaaaaaa".into(),
                }],
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(
            std::error::Error::source(&error).map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let ApplyPruneError::Transport {
            completed_result,
            source,
        } = error;
        assert_eq!(completed_result, None);
        assert_eq!(source.to_string(), "git transport unavailable");
    }
}
