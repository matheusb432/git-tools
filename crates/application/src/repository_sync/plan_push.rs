//! Plans the current repository's push without changing Git state.

use std::path::{Path, PathBuf};

use domain::repository::PendingChanges;

use super::pending_changes;
use crate::{
    ports::GitRunner,
    shared::git::{capture_checked, command_label},
};

/// Requests a read-only push plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPush {
    pub repo: PathBuf,
}

/// Describes the current repository and upstream selected for a push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    pub name: String,
    pub top: PathBuf,
    pub branch: String,
    pub remote: String,
    pub remote_url: String,
    pub pending: PendingChanges,
}

/// Represents either a refused push or a target ready for confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushPlan {
    Refused(String),
    Ready(PushTarget),
}

/// Reports an unexpected Git transport failure while planning a push.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanPushError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a read-only push plan from the repository's local Git state.
///
/// # Errors
///
/// Returns [`PlanPushError`] when Git transport fails.
#[cqrsy::query]
pub fn execute(query: PlanPush, git: &impl GitRunner) -> Result<PushPlan, PlanPushError> {
    let PlanPush { repo } = query;
    let top_args = ["rev-parse", "--show-toplevel"];
    let Some(top) = capture_non_empty(git, &repo, &top_args)? else {
        return Ok(PushPlan::Refused("not a git repo".into()));
    };
    let top = PathBuf::from(top);

    let branch_args = ["rev-parse", "--abbrev-ref", "HEAD"];
    let branch = match capture_non_empty(git, &top, &branch_args)? {
        Some(branch) if branch != "HEAD" => branch,
        _ => {
            return Ok(PushPlan::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
    };

    let remote_key = format!("branch.{branch}.remote");
    let remote_args = ["config", remote_key.as_str()];
    let Some(remote) = capture_non_empty(git, &top, &remote_args)? else {
        return Ok(PushPlan::Refused(format!(
            "no upstream tracking branch (run: git push -u origin {branch})"
        )));
    };

    let remote_url_args = ["remote", "get-url", remote.as_str()];
    let remote_url = capture_checked(git, &top, &remote_url_args)
        .map_err(|source| transport(&remote_url_args, source))?
        .unwrap_or_default();
    let status_args = ["status", "--porcelain"];
    let porcelain = match git
        .run(&top, &status_args)
        .map_err(|source| transport(&status_args, source))?
    {
        output if output.exit_code == 0 => output.stdout,
        _ => return Ok(PushPlan::Refused("git status failed".into())),
    };
    let ahead_args = ["rev-list", "--count", "@{u}..HEAD"];
    let ahead = capture_checked(git, &top, &ahead_args)
        .map_err(|source| transport(&ahead_args, source))?
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or(0);

    Ok(PushPlan::Ready(PushTarget {
        name: repo_name(&top),
        top,
        branch,
        remote,
        remote_url,
        pending: pending_changes::classify(&porcelain, ahead),
    }))
}

fn capture_non_empty(
    git: &impl GitRunner,
    repo: &Path,
    args: &[&str],
) -> Result<Option<String>, PlanPushError> {
    capture_checked(git, repo, args)
        .map(|captured| captured.filter(|value| !value.is_empty()))
        .map_err(|source| transport(args, source))
}

fn transport(args: &[&str], source: anyhow::Error) -> PlanPushError {
    PlanPushError::Transport {
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
    fn detached_head_is_a_refused_push_plan() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repos/api\n"),
            FakeGitRunner::ok("HEAD\n"),
        ]);

        let plan = execute(PlanPush { repo: ".".into() }, &git)
            .expect("a detached head is an expected refusal");

        assert_eq!(
            plan,
            PushPlan::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn missing_upstream_is_a_refused_push_plan() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repos/api\n"),
            FakeGitRunner::ok("feature\n"),
            FakeGitRunner::exit_err("no upstream", 1),
        ]);

        let plan = execute(PlanPush { repo: ".".into() }, &git)
            .expect("a missing upstream is an expected refusal");

        assert_eq!(
            plan,
            PushPlan::Refused(
                "no upstream tracking branch (run: git push -u origin feature)".into()
            )
        );
    }

    #[test]
    fn ready_plan_includes_remote_and_pending_changes() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repos/api\n"),
            FakeGitRunner::ok("main\n"),
            FakeGitRunner::ok("origin\n"),
            FakeGitRunner::ok("git@example.com:team/api.git\n"),
            FakeGitRunner::ok(" M a.txt\n?? b.txt\n"),
            FakeGitRunner::ok("2\n"),
        ]);

        let plan = execute(PlanPush { repo: ".".into() }, &git).expect("push plan is built");

        assert_eq!(
            plan,
            PushPlan::Ready(PushTarget {
                name: "api".into(),
                top: "/repos/api".into(),
                branch: "main".into(),
                remote: "origin".into(),
                remote_url: "git@example.com:team/api.git".into(),
                pending: PendingChanges {
                    changed: 2,
                    staged: 0,
                    unprepared: 2,
                    ahead: 2,
                },
            })
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(PlanPush { repo: ".".into() }, &git)
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
