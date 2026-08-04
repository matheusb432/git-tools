//! Plans the current repository's push without changing Git state.

use std::path::PathBuf;

use gtl_models::repository::PendingChanges;

use crate::{
    ports::{GitClient, GitEffect},
    shared::repository_name::from_path,
};

/// Requests a read-only push plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPush {
    pub repo_path: PathBuf,
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
pub enum PlanPushOk {
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
pub fn execute(query: PlanPush, git: &impl GitClient) -> Result<PlanPushOk, PlanPushError> {
    let PlanPush { repo_path } = query;
    let Some(top) = git
        .discover_top(&repo_path)
        .map_err(|source| transport("discover repository", source))?
    else {
        return Ok(PlanPushOk::Refused("not a git repo".into()));
    };

    let branch = match git
        .current_branch(&top)
        .map_err(|source| transport("read current branch", source))?
    {
        branch if branch != "HEAD" => branch,
        _ => {
            return Ok(PlanPushOk::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
    };

    let Some(remote) = git
        .branch_remote(&top, &branch)
        .map_err(|source| transport("read branch remote", source))?
    else {
        return Ok(PlanPushOk::Refused(format!(
            "no upstream tracking branch (run: git push -u origin {branch})"
        )));
    };

    let remote_url = git
        .remote_url(&top, &remote)
        .map_err(|source| transport("read remote URL", source))?
        .unwrap_or_default();
    let working_tree = match git
        .working_tree(&top)
        .map_err(|source| transport("read working tree", source))?
    {
        GitEffect::Applied(working_tree) => working_tree,
        GitEffect::Rejected(_) => return Ok(PlanPushOk::Refused("git status failed".into())),
    };
    let ahead = git
        .commit_count(&top, "@{u}..HEAD")
        .map_err(|source| transport("count unpushed commits", source))?
        .unwrap_or(0);

    Ok(PlanPushOk::Ready(PushTarget {
        name: from_path(&top),
        top,
        branch,
        remote,
        remote_url,
        pending: PendingChanges {
            changed: working_tree.files.len(),
            staged: working_tree.staged,
            unprepared: working_tree.unprepared,
            ahead,
        },
    }))
}

fn transport(command: &str, source: anyhow::Error) -> PlanPushError {
    PlanPushError::Transport {
        command: command.into(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::ScriptedGitClient;

    #[test]
    fn detached_head_is_a_refused_push_plan() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("HEAD\n"),
        ]);

        let plan = execute(
            PlanPush {
                repo_path: ".".into(),
            },
            &git,
        )
        .expect("a detached head is an expected refusal");

        assert_eq!(
            plan,
            PlanPushOk::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn missing_upstream_is_a_refused_push_plan() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("feature\n"),
            ScriptedGitClient::rejected("no upstream"),
        ]);

        let plan = execute(
            PlanPush {
                repo_path: ".".into(),
            },
            &git,
        )
        .expect("a missing upstream is an expected refusal");

        assert_eq!(
            plan,
            PlanPushOk::Refused(
                "no upstream tracking branch (run: git push -u origin feature)".into()
            )
        );
    }

    #[test]
    fn ready_plan_includes_remote_and_pending_changes() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied("origin\n"),
            ScriptedGitClient::applied("git@example.com:team/api.git\n"),
            ScriptedGitClient::applied(" M a.txt\n?? b.txt\n"),
            ScriptedGitClient::applied("2\n"),
        ]);

        let plan = execute(
            PlanPush {
                repo_path: ".".into(),
            },
            &git,
        )
        .expect("push plan is built");

        assert_eq!(
            plan,
            PlanPushOk::Ready(PushTarget {
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
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            PlanPush {
                repo_path: ".".into(),
            },
            &git,
        )
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
