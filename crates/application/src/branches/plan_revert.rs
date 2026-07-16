//! Plans recovery of the target branch's prior fast-forward position.

use std::path::{Path, PathBuf};

use crate::{
    ports::GitRunner,
    shared::git::{capture_checked, command_label, succeeds_checked},
};

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
pub enum RevertPlan {
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
#[cqrsy::handler(query)]
pub fn execute(query: PlanRevert, git: &impl GitRunner) -> Result<RevertPlan, PlanRevertError> {
    let PlanRevert { repo, onto } = query;
    let top_args = ["rev-parse", "--show-toplevel"];
    let Some(top) = capture_checked(git, &repo, &top_args)
        .map_err(|source| transport(&top_args, source))?
        .filter(|top| !top.is_empty())
        .map(PathBuf::from)
    else {
        return Ok(RevertPlan::Refused("not a git repo".into()));
    };

    let branch_args = ["rev-parse", "--abbrev-ref", "HEAD"];
    match capture_checked(git, &top, &branch_args)
        .map_err(|source| transport(&branch_args, source))?
    {
        Some(branch) if branch == onto => {}
        Some(branch) if branch == "HEAD" => {
            return Ok(RevertPlan::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
        Some(_) => {
            return Ok(RevertPlan::Refused(format!(
                "revert expects to be on '{onto}' (the branch the last sw rebased)"
            )));
        }
        None => return Ok(RevertPlan::Refused("not a git repo".into())),
    }

    let status_args = ["status", "--porcelain"];
    match capture_checked(git, &top, &status_args)
        .map_err(|source| transport(&status_args, source))?
    {
        Some(status) if !status.is_empty() => {
            return Ok(RevertPlan::Refused(
                "working tree not clean — commit or stash first".into(),
            ));
        }
        None => return Ok(RevertPlan::Refused("git status failed".into())),
        Some(_) => {}
    }

    let prior_ref = format!("{onto}@{{1}}");
    let prior_args = ["rev-parse", prior_ref.as_str()];
    let Some(prior_sha) = capture_checked(git, &top, &prior_args)
        .map_err(|source| transport(&prior_args, source))?
        .filter(|prior_sha| !prior_sha.is_empty())
    else {
        return Ok(RevertPlan::Refused(format!(
            "no prior position for '{onto}' in the reflog"
        )));
    };

    if !is_ancestor(git, &top, &prior_ref, &onto)? {
        return Ok(RevertPlan::Refused(format!(
            "'{onto}' moved in a way that isn't a simple fast-forward; refusing to auto-revert"
        )));
    }

    Ok(RevertPlan::Ready(RevertTarget {
        top,
        onto,
        prior_sha,
    }))
}

fn is_ancestor(
    git: &impl GitRunner,
    repo: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool, PlanRevertError> {
    let args = ["merge-base", "--is-ancestor", ancestor, descendant];
    succeeds_checked(git, repo, &args).map_err(|source| transport(&args, source))
}

fn transport(args: &[&str], source: anyhow::Error) -> PlanRevertError {
    PlanRevertError::Transport {
        command: command_label(args),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::FakeGitRunner;

    fn plan(git: &FakeGitRunner) -> RevertPlan {
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
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok("abc123\n"),
            FakeGitRunner::ok(""),
        ]);

        assert_eq!(
            plan(&git),
            RevertPlan::Ready(RevertTarget {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                prior_sha: "abc123".into(),
            })
        );
    }

    #[test]
    fn different_current_branch_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
        ]);

        assert_eq!(
            plan(&git),
            RevertPlan::Refused(
                "revert expects to be on 'main' (the branch the last sw rebased)".into()
            )
        );
    }

    #[test]
    fn detached_head_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("HEAD\n"),
        ]);

        assert_eq!(
            plan(&git),
            RevertPlan::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn dirty_tree_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("main\n"),
            FakeGitRunner::ok(" M a.rs\n"),
        ]);

        assert_eq!(
            plan(&git),
            RevertPlan::Refused("working tree not clean — commit or stash first".into())
        );
    }

    #[test]
    fn missing_prior_reflog_position_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("", 128),
        ]);

        assert_eq!(
            plan(&git),
            RevertPlan::Refused("no prior position for 'main' in the reflog".into())
        );
    }

    #[test]
    fn non_fast_forward_move_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok("abc123\n"),
            FakeGitRunner::exit_err("", 1),
        ]);

        assert_eq!(
            plan(&git),
            RevertPlan::Refused(
                "'main' moved in a way that isn't a simple fast-forward; refusing to auto-revert"
                    .into()
            )
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

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
            "git rev-parse --show-toplevel: git transport unavailable"
        );
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
