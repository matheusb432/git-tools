//! Plans a branch switch without changing repository state.

use std::path::PathBuf;

use crate::{
    ports::GitRunner,
    shared::git::{capture_checked, command_label, onto_exists_checked},
};

/// Requests a read-only switch plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSwitch {
    pub repo: PathBuf,
    pub onto: String,
}

/// Identifies the branch transition ready to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchTarget {
    pub top: PathBuf,
    pub onto: String,
    pub from: String,
}

/// Represents a refused, unnecessary, or ready branch switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchPlan {
    Refused(String),
    Ready(SwitchTarget),
    AlreadyThere(String),
}

/// Reports an unexpected Git transport failure while planning a branch switch.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanSwitchError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a read-only branch switch plan from local Git state.
///
/// # Errors
///
/// Returns [`PlanSwitchError`] when Git transport fails.
#[cqrsy::handler(query)]
pub fn execute(query: PlanSwitch, git: &impl GitRunner) -> Result<SwitchPlan, PlanSwitchError> {
    let PlanSwitch { repo, onto } = query;
    let top_args = ["rev-parse", "--show-toplevel"];
    let Some(top) = capture_checked(git, &repo, &top_args)
        .map_err(|source| transport(&top_args, source))?
        .filter(|top| !top.is_empty())
        .map(PathBuf::from)
    else {
        return Ok(SwitchPlan::Refused("not a git repo".into()));
    };

    let branch_args = ["rev-parse", "--abbrev-ref", "HEAD"];
    let Some(from) = capture_checked(git, &top, &branch_args)
        .map_err(|source| transport(&branch_args, source))?
    else {
        return Ok(SwitchPlan::Refused("not a git repo".into()));
    };
    if from == onto {
        return Ok(SwitchPlan::AlreadyThere(onto));
    }
    if !onto_exists_checked(git, &top, &onto).map_err(|source| {
        let target = format!("refs/heads/{onto}");
        transport(&["rev-parse", "--verify", &target], source)
    })? {
        return Ok(SwitchPlan::Refused(format!(
            "no '{onto}' branch (use --onto <branch>)"
        )));
    }

    Ok(SwitchPlan::Ready(SwitchTarget { top, onto, from }))
}

fn transport(args: &[&str], source: anyhow::Error) -> PlanSwitchError {
    PlanSwitchError::Transport {
        command: command_label(args),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::FakeGitRunner;

    #[test]
    fn ready_plan_reports_target_and_current_branch() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
        ]);

        let plan = execute(
            PlanSwitch {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("switch plan is built");

        assert_eq!(
            plan,
            SwitchPlan::Ready(SwitchTarget {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                from: "feat/x".into(),
            })
        );
    }

    #[test]
    fn target_branch_is_a_closed_noop() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("main\n"),
        ]);

        let plan = execute(
            PlanSwitch {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("the target branch is an expected no-op");

        assert_eq!(plan, SwitchPlan::AlreadyThere("main".into()));
    }

    #[test]
    fn missing_target_branch_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::exit_err("", 128),
        ]);

        let plan = execute(
            PlanSwitch {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("a missing branch is an expected refusal");

        assert_eq!(
            plan,
            SwitchPlan::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(
            PlanSwitch {
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
