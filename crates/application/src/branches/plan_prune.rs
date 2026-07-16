//! Plans deletion of local branches already merged into a target branch.

use std::path::PathBuf;

use crate::ports::GitRunner;

/// Requests a read-only branch-prune plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPrune {
    pub repo: PathBuf,
    pub onto: String,
}

/// Identifies one local branch selected for deletion and recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PruneBranch {
    pub name: String,
    pub sha: String,
}

/// Represents a refused, unnecessary, or ready branch prune.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrunePlan {
    Refused(String),
    Nothing(String),
    Ready {
        top: PathBuf,
        onto: String,
        branches: Vec<PruneBranch>,
    },
}

/// Reports an unexpected Git transport failure while planning branch pruning.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanPruneError {
    /// Git could not be started or its output could not be collected.
    #[error("branch prune planning failed: {source}")]
    Transport {
        /// Preserves the original Git transport failure.
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a read-only branch-prune plan from local Git state.
///
/// # Errors
///
/// Returns [`PlanPruneError`] when Git cannot be executed.
#[cqrsy::handler(query)]
pub fn execute(query: PlanPrune, git: &impl GitRunner) -> Result<PrunePlan, PlanPruneError> {
    let PlanPrune { repo, onto } = query;
    let top = git
        .run(&repo, &["rev-parse", "--show-toplevel"])
        .map_err(|source| PlanPruneError::Transport { source })?;
    if !top.success() || top.stdout.trim().is_empty() {
        return Ok(PrunePlan::Refused("not a git repo".into()));
    }
    let top = PathBuf::from(top.stdout.trim());

    let current = git
        .run(&top, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map_err(|source| PlanPruneError::Transport { source })?;
    let current = match (current.success(), current.stdout.trim()) {
        (true, "HEAD") => {
            return Ok(PrunePlan::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
        (true, branch) => branch.to_string(),
        (false, _) => return Ok(PrunePlan::Refused("not a git repo".into())),
    };

    let target = git
        .run(
            &top,
            &["rev-parse", "--verify", &format!("refs/heads/{onto}")],
        )
        .map_err(|source| PlanPruneError::Transport { source })?;
    if !target.success() {
        return Ok(PrunePlan::Refused(format!(
            "no '{onto}' branch (use --onto <branch>)"
        )));
    }

    let listing = git
        .run(
            &top,
            &[
                "for-each-ref",
                "--merged",
                &onto,
                "--format=%(refname:short) %(objectname:short)",
                "refs/heads/",
            ],
        )
        .map_err(|source| PlanPruneError::Transport { source })?;
    if !listing.success() {
        return Ok(PrunePlan::Refused("git for-each-ref failed".into()));
    }

    let branches = listing
        .stdout
        .lines()
        .filter_map(|line| {
            let (name, sha) = line.trim().split_once(char::is_whitespace)?;
            let name = name.trim();
            let sha = sha.trim();
            if name.is_empty() || name == onto || name == current {
                None
            } else {
                Some(PruneBranch {
                    name: name.into(),
                    sha: sha.into(),
                })
            }
        })
        .collect::<Vec<_>>();

    if branches.is_empty() {
        return Ok(PrunePlan::Nothing(format!(
            "no merged branches to prune (against '{onto}')"
        )));
    }

    Ok(PrunePlan::Ready {
        top,
        onto,
        branches,
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::FakeGitRunner;

    #[test]
    fn plan_excludes_target_and_current_branches() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repo\n"),
            FakeGitRunner::ok("feature/current\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok("main aaaaaaa\nfeature/current bbbbbbb\nfeature/done ccccccc\n"),
        ]);
        let plan = execute(
            PlanPrune {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("a successful Git plan remains a closed value");
        assert!(matches!(plan, PrunePlan::Ready { branches, .. }
        if branches == vec![PruneBranch {
            name: "feature/done".into(),
            sha: "ccccccc".into()
        }]));
    }

    #[test]
    fn plan_reports_nothing_when_only_target_and_current_are_merged() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repo\n"),
            FakeGitRunner::ok("feature/current\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok("main aaaaaaa\nfeature/current bbbbbbb\n"),
        ]);

        let plan = execute(
            PlanPrune {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("nothing to prune remains a closed value");

        assert_eq!(
            plan,
            PrunePlan::Nothing("no merged branches to prune (against 'main')".into())
        );
    }

    #[test]
    fn detached_head_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repo\n"),
            FakeGitRunner::ok("HEAD\n"),
        ]);

        let plan = execute(
            PlanPrune {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("detached HEAD remains a closed refusal");

        assert_eq!(
            plan,
            PrunePlan::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn missing_target_branch_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repo\n"),
            FakeGitRunner::ok("feature/current\n"),
            FakeGitRunner::exit_err("", 128),
        ]);

        let plan = execute(
            PlanPrune {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("a missing target remains a closed refusal");

        assert_eq!(
            plan,
            PrunePlan::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn non_repository_path_is_refused() {
        let git = FakeGitRunner::new(vec![FakeGitRunner::exit_err("", 128)]);

        let plan = execute(
            PlanPrune {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("a non-repository Git exit remains a closed refusal");

        assert_eq!(plan, PrunePlan::Refused("not a git repo".into()));
    }

    #[test]
    fn plan_transport_failure_remains_an_error_with_its_source() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(
            PlanPrune {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect_err("transport failure must remain an error");

        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
    }
}
