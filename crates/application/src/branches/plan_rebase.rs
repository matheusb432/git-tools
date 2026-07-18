//! Plans a safe fast-forward of the target branch to the current feature branch.

use std::path::{Path, PathBuf};

use crate::{
    ports::GitRunner,
    shared::git::{capture_checked, command_label, onto_exists_checked, succeeds_checked},
};

/// Requests a read-only fast-forward plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRebase {
    pub repo: PathBuf,
    pub onto: String,
}

/// Identifies the safe fast-forward ready to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseTarget {
    pub top: PathBuf,
    pub onto: String,
    pub feature: String,
}

impl RebaseTarget {
    /// Returns the Git range containing only commits to promote.
    pub fn range(&self) -> String {
        format!("{}..{}", self.onto, self.feature)
    }
}

/// Represents a refused, unnecessary, or ready fast-forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebasePlan {
    Refused(String),
    Ready(RebaseTarget),
    Noop(String),
}

/// Reports an unexpected Git transport failure while planning a fast-forward.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanRebaseError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Builds a read-only safe fast-forward plan from local Git state.
///
/// # Errors
///
/// Returns [`PlanRebaseError`] when Git transport fails.
#[cqrsy::query]
pub fn execute(query: PlanRebase, git: &impl GitRunner) -> Result<RebasePlan, PlanRebaseError> {
    let PlanRebase { repo, onto } = query;
    let top_args = ["rev-parse", "--show-toplevel"];
    let Some(top) = capture_checked(git, &repo, &top_args)
        .map_err(|source| transport(&top_args, source))?
        .filter(|top| !top.is_empty())
        .map(PathBuf::from)
    else {
        return Ok(RebasePlan::Refused("not a git repo".into()));
    };

    let branch_args = ["rev-parse", "--abbrev-ref", "HEAD"];
    let feature = match capture_checked(git, &top, &branch_args)
        .map_err(|source| transport(&branch_args, source))?
    {
        Some(branch) if branch != "HEAD" => branch,
        Some(_) => {
            return Ok(RebasePlan::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
        None => return Ok(RebasePlan::Refused("not a git repo".into())),
    };
    if feature == onto {
        return Ok(RebasePlan::Refused(format!(
            "already on '{onto}' — nothing to promote"
        )));
    }
    if !onto_exists_checked(git, &top, &onto).map_err(|source| {
        let target = format!("refs/heads/{onto}");
        transport(&["rev-parse", "--verify", &target], source)
    })? {
        return Ok(RebasePlan::Refused(format!(
            "no '{onto}' branch (use --onto <branch>)"
        )));
    }
    let status_args = ["status", "--porcelain"];
    match capture_checked(git, &top, &status_args)
        .map_err(|source| transport(&status_args, source))?
    {
        Some(status) if !status.is_empty() => {
            return Ok(RebasePlan::Refused(
                "working tree not clean — commit or stash first".into(),
            ));
        }
        None => return Ok(RebasePlan::Refused("git status failed".into())),
        Some(_) => {}
    }

    if !is_ancestor(git, &top, &onto, &feature)? {
        let extra = count_range(git, &top, &format!("{feature}..{onto}"))?.unwrap_or(0);
        return Ok(RebasePlan::Refused(format!(
            "'{onto}' has diverged from '{feature}' (+{extra} commits it lacks); fast-forward unsafe — rebase or merge manually"
        )));
    }

    match count_range(git, &top, &format!("{onto}..{feature}"))? {
        Some(0) => {
            return Ok(RebasePlan::Noop(format!(
                "'{onto}' already up to date with '{feature}'"
            )));
        }
        None => return Ok(RebasePlan::Refused("git rev-list failed".into())),
        Some(_) => {}
    }

    Ok(RebasePlan::Ready(RebaseTarget { top, onto, feature }))
}

fn is_ancestor(
    git: &impl GitRunner,
    repo: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool, PlanRebaseError> {
    let args = ["merge-base", "--is-ancestor", ancestor, descendant];
    succeeds_checked(git, repo, &args).map_err(|source| transport(&args, source))
}

fn count_range(
    git: &impl GitRunner,
    repo: &Path,
    range: &str,
) -> Result<Option<usize>, PlanRebaseError> {
    let args = ["rev-list", "--count", range];
    capture_checked(git, repo, &args)
        .map(|captured| captured.and_then(|count| count.parse().ok()))
        .map_err(|source| transport(&args, source))
}

fn transport(args: &[&str], source: anyhow::Error) -> PlanRebaseError {
    PlanRebaseError::Transport {
        command: command_label(args),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::testing::FakeGitRunner;

    fn plan(git: &FakeGitRunner) -> RebasePlan {
        execute(
            PlanRebase {
                repo: ".".into(),
                onto: "main".into(),
            },
            git,
        )
        .expect("rebase plan is built")
    }

    #[test]
    fn ancestor_target_behind_feature_is_ready() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok("3\n"),
        ]);

        assert_eq!(
            plan(&git),
            RebasePlan::Ready(RebaseTarget {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                feature: "feat/x".into(),
            })
        );
    }

    #[test]
    fn target_range_selects_only_promoted_commits() {
        let target = RebaseTarget {
            top: "/repo".into(),
            onto: "main".into(),
            feature: "feature".into(),
        };

        assert_eq!(target.range(), "main..feature");
    }

    #[test]
    fn current_target_branch_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("main\n"),
        ]);

        assert_eq!(
            plan(&git),
            RebasePlan::Refused("already on 'main' — nothing to promote".into())
        );
    }

    #[test]
    fn dirty_tree_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok(" M a.rs\n"),
        ]);

        assert_eq!(
            plan(&git),
            RebasePlan::Refused("working tree not clean — commit or stash first".into())
        );
    }

    #[test]
    fn missing_target_branch_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::exit_err("", 128),
        ]);

        assert_eq!(
            plan(&git),
            RebasePlan::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn diverged_target_is_refused_without_mutation() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/repo\n"),
            FakeGitRunner::ok("feature\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("", 1),
            FakeGitRunner::exit_err("", 1),
        ]);

        let plan = execute(
            PlanRebase {
                repo: ".".into(),
                onto: "main".into(),
            },
            &git,
        )
        .expect("divergence is an expected refusal");

        assert!(matches!(plan, RebasePlan::Refused(detail) if detail.contains("diverged")));
    }

    #[test]
    fn divergence_detail_reports_target_only_commits() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("", 1),
            FakeGitRunner::ok("2\n"),
        ]);

        assert_eq!(
            plan(&git),
            RebasePlan::Refused(
                "'main' has diverged from 'feat/x' (+2 commits it lacks); fast-forward unsafe — rebase or merge manually".into()
            )
        );
    }

    #[test]
    fn up_to_date_target_is_a_closed_noop() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok("0\n"),
        ]);

        assert_eq!(
            plan(&git),
            RebasePlan::Noop("'main' already up to date with 'feat/x'".into())
        );
    }

    #[test]
    fn failed_range_count_is_refused() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("/home/me/repo\n"),
            FakeGitRunner::ok("feat/x\n"),
            FakeGitRunner::ok("refs/heads/main\n"),
            FakeGitRunner::ok(""),
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("", 1),
        ]);

        assert_eq!(
            plan(&git),
            RebasePlan::Refused("git rev-list failed".into())
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git =
            FakeGitRunner::with_results(vec![Err(anyhow::anyhow!("git transport unavailable"))]);

        let error = execute(
            PlanRebase {
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
