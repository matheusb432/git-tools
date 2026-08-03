//! Plans a branch switch without changing repository state.

use std::path::PathBuf;

use crate::ports::GitClient;

/// Requests a read-only switch plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSwitch {
    pub repo_path: PathBuf,
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
pub enum PlanSwitchOk {
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
#[cqrsy::query]
pub fn execute(query: PlanSwitch, git: &impl GitClient) -> Result<PlanSwitchOk, PlanSwitchError> {
    let PlanSwitch { repo_path, onto } = query;
    let Some(top) = git
        .discover_top(&repo_path)
        .map_err(|source| transport("discover repository", source))?
    else {
        return Ok(PlanSwitchOk::Refused("not a git repo".into()));
    };

    let from = git
        .current_branch(&top)
        .map_err(|source| transport("read current branch", source))?;
    if from == onto {
        return Ok(PlanSwitchOk::AlreadyThere(onto));
    }
    if !git
        .revision_exists(&top, &format!("refs/heads/{onto}"))
        .map_err(|source| transport("find target branch", source))?
    {
        return Ok(PlanSwitchOk::Refused(format!(
            "no '{onto}' branch (use --onto <branch>)"
        )));
    }

    Ok(PlanSwitchOk::Ready(SwitchTarget { top, onto, from }))
}

fn transport(command: &str, source: anyhow::Error) -> PlanSwitchError {
    PlanSwitchError::Transport {
        command: command.to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::ScriptedGitClient;

    #[test]
    fn ready_plan_reports_target_and_current_branch() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
        ]);

        let plan = execute(
            PlanSwitch {
                repo_path: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("switch plan is built");

        assert_eq!(
            plan,
            PlanSwitchOk::Ready(SwitchTarget {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                from: "feat/x".into(),
            })
        );
    }

    #[test]
    fn target_branch_is_a_closed_noop() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("main\n"),
        ]);

        let plan = execute(
            PlanSwitch {
                repo_path: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("the target branch is an expected no-op");

        assert_eq!(plan, PlanSwitchOk::AlreadyThere("main".into()));
    }

    #[test]
    fn missing_target_branch_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::rejected(""),
        ]);

        let plan = execute(
            PlanSwitch {
                repo_path: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("a missing branch is an expected refusal");

        assert_eq!(
            plan,
            PlanSwitchOk::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = execute(
            PlanSwitch {
                repo_path: ".".into(),
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
