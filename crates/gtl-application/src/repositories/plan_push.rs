//! Plans the current repository's push without changing Git state.

use std::path::PathBuf;

use gtl_models::{
    git::{BranchName, CommitCount, GitHead, GitRange, RemoteName, RemoteUrl},
    paths::{ProjectName, RepositoryRoot},
    repository::{PathCount, PendingChanges},
};

use crate::ports::{GitClient, GitEffect};

/// Describes the current repository and upstream selected for a push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    pub name: ProjectName,
    pub top: RepositoryRoot,
    pub branch: BranchName,
    pub remote: RemoteName,
    pub remote_url: Option<RemoteUrl>,
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
#[expect(
    clippy::needless_pass_by_value,
    reason = "CQRsy operations own their request value"
)]
pub fn execute(repo_path: PathBuf, git: &impl GitClient) -> Result<PlanPushOk, PlanPushError> {
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
        GitHead::Branch(branch) => branch,
        GitHead::Detached => {
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
        .map_err(|source| transport("read remote URL", source))?;
    let working_tree = match git
        .working_tree(&top)
        .map_err(|source| transport("read working tree", source))?
    {
        GitEffect::Applied(working_tree) => working_tree,
        GitEffect::Rejected(_) => return Ok(PlanPushOk::Refused("git status failed".into())),
    };
    let ahead = git
        .commit_count(&top, &GitRange::upstream_to_head())
        .map_err(|source| transport("count unpushed commits", source))?
        .unwrap_or(CommitCount::default());

    Ok(PlanPushOk::Ready(PushTarget {
        name: top.project_name(),
        top,
        branch,
        remote,
        remote_url,
        pending: PendingChanges {
            changed: PathCount::from_len(working_tree.files.len()),
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
    use crate::{
        repositories::plan_push,
        utils::{ScriptedGitClient, branch_name, remote_name},
    };

    #[test]
    fn detached_head_is_a_refused_push_plan() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repos/api\n"),
            ScriptedGitClient::applied("HEAD\n"),
        ]);

        let plan =
            plan_push::execute(".".into(), &git).expect("a detached head is an expected refusal");

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

        let plan = plan_push::execute(".".into(), &git)
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

        let plan = plan_push::execute(".".into(), &git).expect("push plan is built");

        assert_eq!(
            plan,
            PlanPushOk::Ready(PushTarget {
                name: crate::utils::project_name("api"),
                top: crate::utils::repository_root("/repos/api"),
                branch: branch_name("main"),
                remote: remote_name("origin"),
                remote_url: Some(RemoteUrl::try_new("git@example.com:team/api.git").unwrap()),
                pending: PendingChanges {
                    changed: PathCount::new(2),
                    staged: PathCount::default(),
                    unprepared: PathCount::new(2),
                    ahead: CommitCount::new(2),
                },
            })
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = plan_push::execute(".".into(), &git)
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
