//! Plans a local-only commit without changing Git state.

use std::path::{Path, PathBuf};

use super::PendingChanges;
use crate::{
    ports::GitRunner,
    shared::git::{capture_checked, command_label},
};

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
pub enum CommitPlan {
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
#[cqrsy::handler(query)]
pub fn execute(query: PlanCommit, git: &impl GitRunner) -> Result<CommitPlan, PlanCommitError> {
    let PlanCommit { repo } = query;
    let top_args = ["rev-parse", "--show-toplevel"];
    let Some(top) = capture_non_empty(git, &repo, &top_args)? else {
        return Ok(CommitPlan::Refused("not a git repo".into()));
    };
    let top = PathBuf::from(top);

    let branch_args = ["rev-parse", "--abbrev-ref", "HEAD"];
    let branch = match capture_non_empty(git, &top, &branch_args)? {
        Some(branch) if branch != "HEAD" => branch,
        _ => {
            return Ok(CommitPlan::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
    };

    let status_args = ["status", "--porcelain"];
    let porcelain = match git
        .run(&top, &status_args)
        .map_err(|source| transport(&status_args, source))?
    {
        output if output.exit_code == 0 => output.stdout,
        _ => return Ok(CommitPlan::Refused("git status failed".into())),
    };

    Ok(CommitPlan::Ready(CommitTarget {
        name: repo_name(&top),
        top,
        branch,
        pending: PendingChanges::from_porcelain(&porcelain, 0),
    }))
}

fn capture_non_empty(
    git: &impl GitRunner,
    repo: &Path,
    args: &[&str],
) -> Result<Option<String>, PlanCommitError> {
    capture_checked(git, repo, args)
        .map(|captured| captured.filter(|value| !value.is_empty()))
        .map_err(|source| transport(args, source))
}

fn transport(args: &[&str], source: anyhow::Error) -> PlanCommitError {
    PlanCommitError::Transport {
        command: command_label(args),
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
    use crate::testing::FakeGitRunner;

    #[test]
    fn commit_plan_does_not_require_an_upstream() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repos/api\n"),
            FakeGitRunner::ok("main\n"),
            FakeGitRunner::ok(" M src/lib.rs\n"),
        ]);

        let plan = execute(PlanCommit { repo: ".".into() }, &git).expect("commit plan is built");

        assert_eq!(
            plan,
            CommitPlan::Ready(CommitTarget {
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
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(PlanCommit { repo: ".".into() }, &git)
            .expect_err("transport failure must remain an error");

        assert_eq!(
            error.to_string(),
            "git rev-parse --show-toplevel: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
