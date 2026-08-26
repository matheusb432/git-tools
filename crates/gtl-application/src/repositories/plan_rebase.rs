//! Plans a safe fast-forward of the target branch to the current feature branch.

use std::path::PathBuf;

use gtl_models::{
    git::{BranchName, CommitCount, GitHead, GitRange, GitRevision},
    paths::RepositoryRoot,
};

use crate::ports::{GitClient, GitEffect};

/// Requests a read-only fast-forward plan for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRebase {
    pub repo_path: PathBuf,
    pub onto: BranchName,
}

/// Identifies the safe fast-forward ready to apply.
#[derive(Debug, PartialEq, Eq)]
pub struct RebaseTarget {
    pub top: RepositoryRoot,
    pub onto: BranchName,
    pub feature: BranchName,
}

impl RebaseTarget {
    /// Returns the Git range containing only commits to promote.
    pub fn range(&self) -> GitRange {
        GitRange::two_dot(
            &GitRevision::from(&self.onto),
            &GitRevision::from(&self.feature),
        )
    }
}

/// Represents a refused, unnecessary, or ready fast-forward.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanRebaseOk {
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
pub fn execute(query: PlanRebase, git: &impl GitClient) -> Result<PlanRebaseOk, PlanRebaseError> {
    let PlanRebase { repo_path, onto } = query;
    let Some(top) = git
        .discover_top(&repo_path)
        .map_err(|source| transport("discover repository", source))?
    else {
        return Ok(PlanRebaseOk::Refused("not a git repo".into()));
    };

    let feature = match git
        .current_branch(&top)
        .map_err(|source| transport("read current branch", source))?
    {
        GitHead::Branch(branch) => branch,
        GitHead::Detached => {
            return Ok(PlanRebaseOk::Refused(
                "detached HEAD — checkout a branch first".into(),
            ));
        }
    };
    if feature == onto {
        return Ok(PlanRebaseOk::Refused(format!(
            "already on '{onto}' — nothing to promote"
        )));
    }
    if !git
        .revision_exists(&top, &GitRevision::local_branch(&onto))
        .map_err(|source| transport("find target branch", source))?
    {
        return Ok(PlanRebaseOk::Refused(format!(
            "no '{onto}' branch (use --onto <branch>)"
        )));
    }
    match git
        .working_tree(&top)
        .map_err(|source| transport("read working tree", source))?
    {
        GitEffect::Applied(tree) if !tree.files.is_empty() => {
            return Ok(PlanRebaseOk::Refused(
                "working tree not clean — commit or stash first".into(),
            ));
        }
        GitEffect::Rejected(_) => return Ok(PlanRebaseOk::Refused("git status failed".into())),
        GitEffect::Applied(_) => {}
    }

    let onto_revision = GitRevision::from(&onto);
    let feature_revision = GitRevision::from(&feature);
    if !is_ancestor(git, &top, &onto_revision, &feature_revision)? {
        let extra = count_range(
            git,
            &top,
            &GitRange::two_dot(&feature_revision, &onto_revision),
        )?
        .unwrap_or(CommitCount::default());
        return Ok(PlanRebaseOk::Refused(format!(
            "'{onto}' has diverged from '{feature}' (+{extra} commits it lacks); fast-forward unsafe — rebase or merge manually"
        )));
    }

    match count_range(
        git,
        &top,
        &GitRange::two_dot(&onto_revision, &feature_revision),
    )? {
        Some(count) if count == CommitCount::default() => {
            return Ok(PlanRebaseOk::Noop(format!(
                "'{onto}' already up to date with '{feature}'"
            )));
        }
        None => return Ok(PlanRebaseOk::Refused("git rev-list failed".into())),
        Some(_) => {}
    }

    Ok(PlanRebaseOk::Ready(RebaseTarget { top, onto, feature }))
}

fn is_ancestor(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    ancestor: &GitRevision,
    descendant: &GitRevision,
) -> Result<bool, PlanRebaseError> {
    git.is_ancestor(repo_path, ancestor, descendant)
        .map_err(|source| transport("check ancestry", source))
}

fn count_range(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
    range: &GitRange,
) -> Result<Option<CommitCount>, PlanRebaseError> {
    git.commit_count(repo_path, range)
        .map_err(|source| transport("count commits", source))
}

fn transport(command: &str, source: anyhow::Error) -> PlanRebaseError {
    PlanRebaseError::Transport {
        command: command.to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{
        repositories::plan_rebase,
        utils::{ScriptedGitClient, branch_name},
    };

    fn plan(git: &ScriptedGitClient) -> PlanRebaseOk {
        plan_rebase::execute(
            PlanRebase {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            git,
        )
        .expect("rebase plan is built")
    }

    #[test]
    fn ancestor_target_behind_feature_is_ready() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("3\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRebaseOk::Ready(RebaseTarget {
                top: crate::utils::repository_root("/home/me/repo"),
                onto: branch_name("main"),
                feature: branch_name("feat/x"),
            })
        );
    }

    #[test]
    fn target_range_selects_only_promoted_commits() {
        let target = RebaseTarget {
            top: crate::utils::repository_root("/repo"),
            onto: branch_name("main"),
            feature: branch_name("feature"),
        };

        assert_eq!(target.range().as_ref(), "main..feature");
    }

    #[test]
    fn current_target_branch_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("main\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRebaseOk::Refused("already on 'main' — nothing to promote".into())
        );
    }

    #[test]
    fn dirty_tree_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(" M a.rs\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRebaseOk::Refused("working tree not clean — commit or stash first".into())
        );
    }

    #[test]
    fn missing_target_branch_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::rejected(""),
        ]);

        assert_eq!(
            plan(&git),
            PlanRebaseOk::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn diverged_target_is_refused_without_mutation() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo\n"),
            ScriptedGitClient::applied("feature\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected(""),
            ScriptedGitClient::rejected(""),
        ]);

        let plan = plan_rebase::execute(
            PlanRebase {
                repo_path: ".".into(),
                onto: branch_name("main"),
            },
            &git,
        )
        .expect("divergence is an expected refusal");

        assert!(matches!(plan, PlanRebaseOk::Refused(detail) if detail.contains("diverged")));
    }

    #[test]
    fn divergence_detail_reports_target_only_commits() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected(""),
            ScriptedGitClient::applied("2\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRebaseOk::Refused(
                "'main' has diverged from 'feat/x' (+2 commits it lacks); fast-forward unsafe — rebase or merge manually".into()
            )
        );
    }

    #[test]
    fn up_to_date_target_is_a_closed_noop() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied("0\n"),
        ]);

        assert_eq!(
            plan(&git),
            PlanRebaseOk::Noop("'main' already up to date with 'feat/x'".into())
        );
    }

    #[test]
    fn failed_range_count_is_refused() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/home/me/repo\n"),
            ScriptedGitClient::applied("feat/x\n"),
            ScriptedGitClient::applied("refs/heads/main\n"),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected(""),
        ]);

        assert_eq!(
            plan(&git),
            PlanRebaseOk::Refused("git rev-list failed".into())
        );
    }

    #[test]
    fn transport_failure_remains_a_sourced_plan_error() {
        let git = ScriptedGitClient::with_results(vec![Err(anyhow::anyhow!(
            "git transport unavailable"
        ))]);

        let error = plan_rebase::execute(
            PlanRebase {
                repo_path: ".".into(),
                onto: branch_name("main"),
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
