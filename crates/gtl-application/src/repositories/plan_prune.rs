//! Plans deletion of local branches already merged into a target branch.

use std::path::PathBuf;

use gtl_models::{
    diffs::CommitId,
    git::{BranchName, GitHead, GitRevision},
    paths::RepositoryRoot,
};

use crate::ports::{GitClient, GitEffect};

/// Requests a read-only branch-prune plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPrune {
    pub repo_path: PathBuf,
    pub onto: BranchName,
}

/// Requests a branch-prune plan for an already-resolved repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanResolvedPrune {
    pub repo_root: RepositoryRoot,
    pub onto: BranchName,
}

/// Identifies one local branch selected for deletion and recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneBranch {
    pub name: BranchName,
    pub id: CommitId,
}

/// Represents a refused, unnecessary, or ready branch prune.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanPruneOk {
    Refused(String),
    Nothing(String),
    Ready {
        top: RepositoryRoot,
        onto: BranchName,
        branches: Vec<PruneBranch>,
    },
}

/// Reports an unexpected Git transport failure while planning branch pruning.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanPruneError {
    /// Git could not be started or its output could not be collected.
    #[error("branch prune planning failed: {source}")]
    Transport {
        /// Preserves the original Git transport failure.
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a read-only branch-prune plan from local Git state.
///
/// # Errors
///
/// Returns [`PlanPruneError`] when Git cannot be executed.
#[cqrsy::query]
pub fn execute(query: PlanPrune, git: &impl GitClient) -> Result<PlanPruneOk, PlanPruneError> {
    let PlanPrune { repo_path, onto } = query;
    let top = git
        .discover_top(&repo_path)
        .map_err(|source| PlanPruneError::Transport { source })?;
    let Some(top) = top else {
        return Ok(PlanPruneOk::Refused("not a git repo".into()));
    };

    plan_top(top, onto, git)
}

/// Builds a branch-prune plan without discarding a resolved repository identity.
pub fn execute_resolved(
    query: PlanResolvedPrune,
    git: &impl GitClient,
) -> Result<PlanPruneOk, PlanPruneError> {
    let PlanResolvedPrune { repo_root, onto } = query;
    let top = git
        .discover_top(repo_root.as_ref())
        .map_err(|source| PlanPruneError::Transport { source })?;
    let Some(top) = top else {
        return Ok(PlanPruneOk::Refused("not a git repo".into()));
    };

    plan_top(top, onto, git)
}

fn plan_top(
    top: RepositoryRoot,
    onto: BranchName,
    git: &impl GitClient,
) -> Result<PlanPruneOk, PlanPruneError> {
    let current = git
        .current_branch(&top)
        .map_err(|source| PlanPruneError::Transport { source })?;
    let current = match current {
        GitHead::Detached => {
            return Ok(PlanPruneOk::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
        GitHead::Branch(branch) => branch,
    };

    let target = git
        .revision_exists(&top, &GitRevision::local_branch(&onto))
        .map_err(|source| PlanPruneError::Transport { source })?;
    if !target {
        return Ok(PlanPruneOk::Refused(format!(
            "no '{onto}' branch (use --onto <branch>)"
        )));
    }

    let listing = git
        .merged_branches(&top, &GitRevision::from(&onto))
        .map_err(|source| PlanPruneError::Transport { source })?;
    let GitEffect::Applied(listing) = listing else {
        return Ok(PlanPruneOk::Refused("git for-each-ref failed".into()));
    };
    let branches = listing
        .into_iter()
        .filter_map(|branch| {
            if branch.name == onto || branch.name == current {
                None
            } else {
                Some(PruneBranch {
                    name: branch.name,
                    id: branch.id,
                })
            }
        })
        .collect::<Vec<_>>();

    if branches.is_empty() {
        return Ok(PlanPruneOk::Nothing(format!(
            "no merged branches to prune (against '{onto}')"
        )));
    }

    Ok(PlanPruneOk::Ready {
        top,
        onto,
        branches,
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{
        repositories::plan_prune,
        utils::{ScriptedGitClient, branch_name},
    };

    #[test]
    fn plan_excludes_target_and_current_branches() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("feature/current\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(
                "main aaaaaaa\nfeature/current bbbbbbb\nfeature/done ccccccc\n",
            ),
        ]);
        let plan = plan_prune::execute(
            PlanPrune {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            &git,
        )
        .expect("a successful Git plan remains a closed value");
        assert!(matches!(plan, PlanPruneOk::Ready { branches, .. }
        if branches == vec![PruneBranch {
            name: branch_name("feature/done"),
            id: crate::utils::commit_id_fixture("ccccccc")
        }]));
    }

    #[test]
    fn plan_reports_nothing_when_only_target_and_current_are_merged() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("feature/current\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied("main aaaaaaa\nfeature/current bbbbbbb\n"),
        ]);

        let plan = plan_prune::execute(
            PlanPrune {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            &git,
        )
        .expect("nothing to prune remains a closed value");

        assert_eq!(
            plan,
            PlanPruneOk::Nothing("no merged branches to prune (against 'main')".into())
        );
    }

    #[test]
    fn detached_head_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("HEAD\n"),
        ]);

        let plan = plan_prune::execute(
            PlanPrune {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            &git,
        )
        .expect("detached HEAD remains a closed refusal");

        assert_eq!(
            plan,
            PlanPruneOk::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn missing_target_branch_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("feature/current\n"),
            ScriptedGitClient::rejected(""),
        ]);

        let plan = plan_prune::execute(
            PlanPrune {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            &git,
        )
        .expect("a missing target remains a closed refusal");

        assert_eq!(
            plan,
            PlanPruneOk::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn non_repository_path_is_refused() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("")]);

        let plan = plan_prune::execute(
            PlanPrune {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            &git,
        )
        .expect("a non-repository Git exit remains a closed refusal");

        assert_eq!(plan, PlanPruneOk::Refused("not a git repo".into()));
    }

    #[test]
    fn plan_transport_failure_remains_an_error_with_its_source() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = plan_prune::execute(
            PlanPrune {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
