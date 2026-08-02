//! Plans a local-only commit without changing Git state.

use std::path::{Path, PathBuf};

use gtl_models::repository::PendingChanges;

use crate::ports::{GitClient, GitEffect};

/// Requests a read-only local commit plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCommit {
    pub repo: PathBuf,
}

/// Describes the current repository selected for a local-only commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTarget {
    pub name: String,
    pub top: PathBuf,
    pub branch: String,
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
pub fn execute(query: PlanCommit, git: &impl GitClient) -> Result<PlanCommitOk, PlanCommitError> {
    let PlanCommit { repo } = query;
    let Some(top) = git
        .discover_top(&repo)
        .map_err(|source| transport("discover repository", source))?
    else {
        return Ok(PlanCommitOk::Refused("not a git repo".into()));
    };

    let branch = match git
        .current_branch(&top)
        .map_err(|source| transport("read current branch", source))?
    {
        branch if branch != "HEAD" => branch,
        _ => {
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
        name: repo_name(&top),
        top,
        branch,
        pending: PendingChanges {
            changed: working_tree.files.len(),
            staged: working_tree.staged,
            unprepared: working_tree.unprepared,
            ahead: 0,
        },
    }))
}

fn transport(command: &str, source: anyhow::Error) -> PlanCommitError {
    PlanCommitError::Transport {
        command: command.into(),
        source,
    }
}

fn repo_name(top: &Path) -> String {
    top.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::ScriptedGitClient;

    #[test]
    fn commit_plan_does_not_require_an_upstream() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(" M src/lib.rs\n"),
        ]);

        let plan = execute(PlanCommit { repo: ".".into() }, &git).expect("commit plan is built");

        assert_eq!(
            plan,
            PlanCommitOk::Ready(CommitTarget {
                name: "api".into(),
                top: "/repos/api".into(),
                branch: "main".into(),
                pending: PendingChanges {
                    changed: 1,
                    staged: 0,
                    unprepared: 1,
                    ahead: 0,
                },
            })
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(PlanCommit { repo: ".".into() }, &git)
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
