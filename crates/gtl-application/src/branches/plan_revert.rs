//! Plans recovery of the target branch's prior fast-forward position.

use std::path::{Path, PathBuf};

use crate::ports::{GitClient, GitEffect};

/// Requests a read-only branch recovery plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRevert {
    pub repo: PathBuf,
    pub onto: String,
}

/// Identifies the prior branch position ready to restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertTarget {
    pub top: PathBuf,
    pub onto: String,
    pub prior_sha: String,
}

/// Represents a refused or ready branch recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanRevertOk {
    Refused(String),
    Ready(RevertTarget),
}

/// Reports an unexpected Git transport failure while planning branch recovery.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanRevertError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a read-only branch recovery plan from local Git state.
///
/// # Errors
///
/// Returns [`PlanRevertError`] when Git transport fails.
#[cqrsy::query]
pub fn execute(query: PlanRevert, git: &impl GitClient) -> Result<PlanRevertOk, PlanRevertError> {
    let PlanRevert { repo, onto } = query;
    let Some(top) = git
        .discover_top(&repo)
        .map_err(|source| transport("discover repository", source))?
    else {
        return Ok(PlanRevertOk::Refused("not a git repo".into()));
    };

    match git
        .current_branch(&top)
        .map_err(|source| transport("read current branch", source))?
    {
        branch if branch == onto => {}
        branch if branch == "HEAD" => {
            return Ok(PlanRevertOk::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
        _ => {
            return Ok(PlanRevertOk::Refused(format!(
                "revert expects to be on '{onto}' (the branch the last sw rebased)"
            )));
        }
    }

    match git
        .working_tree(&top)
        .map_err(|source| transport("read working tree", source))?
    {
        GitEffect::Applied(tree) if !tree.files.is_empty() => {
            return Ok(PlanRevertOk::Refused(
                "working tree not clean — commit or stash first".into(),
            ));
        }
        GitEffect::Rejected(_) => return Ok(PlanRevertOk::Refused("git status failed".into())),
        GitEffect::Applied(_) => {}
    }

    let prior_ref = format!("{onto}@{{1}}");
    let Some(prior_sha) = git.resolve_sha(&top, &prior_ref).ok() else {
        return Ok(PlanRevertOk::Refused(format!(
            "no prior position for '{onto}' in the reflog"
        )));
    };

    if !is_ancestor(git, &top, &prior_ref, &onto)? {
        return Ok(PlanRevertOk::Refused(format!(
            "'{onto}' moved in a way that isn't a simple fast-forward; refusing to auto-revert"
        )));
    }

    Ok(PlanRevertOk::Ready(RevertTarget {
        top,
        onto,
        prior_sha,
    }))
}

fn is_ancestor(
    git: &impl GitClient,
    repo: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool, PlanRevertError> {
    git.is_ancestor(repo, ancestor, descendant)
        .map_err(|source| transport("check ancestry", source))
}

fn transport(command: &str, source: anyhow::Error) -> PlanRevertError {
    PlanRevertError::Transport {
        command: command.to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::ScriptedGitClient;

    fn plan(git: &ScriptedGitClient) -> PlanRevertOk {
        execute(
            PlanRevert {
                repo: ".".into(),
                onto: "main".into(),
            },
            git,
        )
        .expect("revert plan is built")
    }

    #[test]
    fn prior_fast_forward_position_is_ready() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("abc123\n"),
            ScriptedGitClient::applied(""),
        ]);

        assert_eq!(
            plan(&git),
            PlanRevertOk::Ready(RevertTarget {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                prior_sha: "abc123".into(),
            })
        );
    }

    #[test]
    fn different_current_branch_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRevertOk::Refused(
                "revert expects to be on 'main' (the branch the last sw rebased)".into()
            )
        );
    }

    #[test]
    fn detached_head_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("HEAD\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRevertOk::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn dirty_tree_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(" M a.rs\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRevertOk::Refused("working tree not clean — commit or stash first".into())
        );
    }

    #[test]
    fn missing_prior_reflog_position_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected(""),
        ]);

        assert_eq!(
            plan(&git),
            PlanRevertOk::Refused("no prior position for 'main' in the reflog".into())
        );
    }

    #[test]
    fn non_fast_forward_move_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("abc123\n"),
            ScriptedGitClient::rejected(""),
        ]);

        assert_eq!(
            plan(&git),
            PlanRevertOk::Refused(
                "'main' moved in a way that isn't a simple fast-forward; refusing to auto-revert"
                    .into()
            )
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            PlanRevert {
                repo: ".".into(),
                onto: "main".into(),
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
