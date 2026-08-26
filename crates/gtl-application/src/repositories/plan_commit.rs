//! Plans a local-only commit without changing Git state.

use std::path::Path;

use gtl_models::{
    git::{BranchName, CommitCount, GitHead},
    paths::{ProjectName, RepositoryRoot},
    repository::{PathCount, PendingChanges},
};

use crate::ports::{GitClient, GitEffect};

/// Describes the current repository selected for a local-only commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTarget {
    pub name: ProjectName,
    pub top: RepositoryRoot,
    pub branch: BranchName,
    pub pending: PendingChanges,
}

/// Represents either a refused commit or a target ready for confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanCommitOk {
    Refused(String),
    Ready(CommitTarget),
}

/// Reports an unexpected Git transport failure while planning a local commit.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanCommitError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a read-only local commit plan from the repository's local Git state.
///
/// # Errors
///
/// Returns [`PlanCommitError`] when Git transport fails.
#[cqrsy::query]
pub fn execute(repo_path: &Path, git: &impl GitClient) -> Result<PlanCommitOk, PlanCommitError> {
    let Some(top) = git
        .discover_top(repo_path)
        .map_err(|source| transport("discover repository", source))?
    else {
        return Ok(PlanCommitOk::Refused("not a git repo".into()));
    };

    let branch = match git
        .current_branch(&top)
        .map_err(|source| transport("read current branch", source))?
    {
        GitHead::Branch(branch) => branch,
        GitHead::Detached => {
            return Ok(PlanCommitOk::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
    };

    let working_tree = match git
        .working_tree(&top)
        .map_err(|source| transport("read working tree", source))?
    {
        GitEffect::Applied(working_tree) => working_tree,
        GitEffect::Rejected(_) => return Ok(PlanCommitOk::Refused("git status failed".into())),
    };

    Ok(PlanCommitOk::Ready(CommitTarget {
        name: top.project_name(),
        top,
        branch,
        pending: PendingChanges {
            changed: PathCount::from_len(working_tree.files.len()),
            staged: working_tree.staged,
            unprepared: working_tree.unprepared,
            ahead: CommitCount::default(),
        },
    }))
}

fn transport(command: &str, source: anyhow::Error) -> PlanCommitError {
    PlanCommitError::Transport {
        command: command.into(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{
        repositories::plan_commit,
        utils::{ScriptedGitClient, branch_name},
    };

    #[test]
    fn commit_plan_does_not_require_an_upstream() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(" M src/lib.rs\n"),
        ]);

        let plan = plan_commit::execute(Path::new("."), &git).expect("commit plan is built");

        assert_eq!(
            plan,
            PlanCommitOk::Ready(CommitTarget {
                name: crate::utils::project_name("api"),
                top: crate::utils::repository_root("/repos/api"),
                branch: branch_name("main"),
                pending: PendingChanges {
                    changed: PathCount::new(1),
                    staged: PathCount::default(),
                    unprepared: PathCount::new(1),
                    ahead: CommitCount::default(),
                },
            })
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = plan_commit::execute(Path::new("."), &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(
            error.to_string(),
            "discover repository: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
