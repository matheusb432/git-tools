//! Applies a planned fast-forward and reports the promoted commits.

use std::fmt::Write as _;

use super::{branch_recovery::BranchRecovery, plan_rebase::RebaseTarget};
use crate::ports::{GitClient, GitEffect};

/// Requests applying one confirmed fast-forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyRebase {
    pub target: RebaseTarget,
}

/// Classifies the result of applying a fast-forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseStatus {
    FastForwarded,
    Failed,
}

/// Reports the closed fast-forward status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyRebaseOk {
    pub status: RebaseStatus,
    pub detail: String,
    pub progress: RebaseProgress,
}

/// Reports completed fast-forward steps and available recovery data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RebaseProgress {
    pub branch_switched: bool,
    pub promoted_commits: Option<Vec<PromotedCommit>>,
    pub fast_forwarded: bool,
    pub recovery: Option<BranchRecovery>,
}

impl ApplyRebaseOk {
    fn new(status: RebaseStatus, detail: impl Into<String>, progress: RebaseProgress) -> Self {
        Self {
            status,
            detail: detail.into(),
            progress,
        }
    }
}

/// Identifies one feature commit promoted by the fast-forward.
///
/// # Examples
///
/// ```
/// use application::branches::apply_rebase::PromotedCommit;
///
/// let commit = PromotedCommit {
///     sha: "abc123456".into(),
///     subject: "feat: add progress".into(),
/// };
/// assert_eq!(commit.sha, "abc123456");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotedCommit {
    /// Stores the abbreviated commit identity displayed in the result detail.
    pub sha: String,
    /// Stores the commit subject displayed beside the identity.
    pub subject: String,
}

/// Reports an unexpected Git transport failure while applying a fast-forward.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyRebaseError {
    /// Git could not be started or its output could not be collected.
    #[error("{command}: {source}")]
    Transport {
        command: String,
        progress: RebaseProgress,
        #[source]
        source: anyhow::Error,
    },
}

/// Switches to the target branch and fast-forwards it to the feature tip.
///
/// # Errors
///
/// Returns [`ApplyRebaseError`] when Git transport fails.
#[cqrsy::command]
pub fn execute(
    command: ApplyRebase,
    git: &impl GitClient,
) -> Result<ApplyRebaseOk, ApplyRebaseError> {
    let ApplyRebase { target } = command;
    let mut progress = RebaseProgress::default();
    match git.switch(&target.top, &target.onto) {
        Ok(GitEffect::Applied(())) => {}
        Ok(GitEffect::Rejected(detail)) => {
            return Ok(ApplyRebaseOk::new(RebaseStatus::Failed, detail, progress));
        }
        Err(source) => return Err(transport("switch branch", progress, source)),
    }
    progress.branch_switched = true;
    progress.recovery = Some(BranchRecovery::switch_to(&target.feature));

    // The range becomes empty after the fast-forward, so select promoted commits first.
    let commits = promoted_commits(git, &target, &progress)?;
    progress.promoted_commits.clone_from(&commits);
    match git.fast_forward(&target.top, &target.feature) {
        Ok(GitEffect::Applied(_)) => {
            progress.fast_forwarded = true;
            progress.recovery = None;
            Ok(ApplyRebaseOk::new(
                RebaseStatus::FastForwarded,
                rebase_log(&target, commits.as_deref().unwrap_or_default()),
                progress,
            ))
        }
        Ok(GitEffect::Rejected(detail)) => {
            Ok(ApplyRebaseOk::new(RebaseStatus::Failed, detail, progress))
        }
        Err(source) => Err(transport("fast-forward branch", progress, source)),
    }
}

fn promoted_commits(
    git: &impl GitClient,
    target: &RebaseTarget,
    progress: &RebaseProgress,
) -> Result<Option<Vec<PromotedCommit>>, ApplyRebaseError> {
    let range = target.range();
    match git
        .brief_log(&target.top, &range)
        .map_err(|source| transport("read promoted commits", progress.clone(), source))?
    {
        GitEffect::Rejected(_) => Ok(None),
        GitEffect::Applied(lines) => Ok(Some(
            lines
                .into_iter()
                .filter_map(|line| {
                    let (sha, subject) = line.split_once(char::is_whitespace)?;
                    Some(PromotedCommit {
                        sha: sha.to_string(),
                        subject: subject.trim().to_string(),
                    })
                })
                .collect(),
        )),
    }
}

fn transport(command: &str, progress: RebaseProgress, source: anyhow::Error) -> ApplyRebaseError {
    ApplyRebaseError::Transport {
        command: command.to_string(),
        progress,
        source,
    }
}

fn rebase_log(target: &RebaseTarget, commits: &[PromotedCommit]) -> String {
    let mut detail = format!(
        "switched to '{}' from '{}'\nfast-forwarded {} +{} commits:",
        target.onto,
        target.feature,
        target.onto,
        commits.len()
    );
    for commit in commits {
        let _ = write!(detail, "\n  {}  {}", commit.sha, commit.subject);
    }
    detail
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{branches::plan_rebase::RebaseTarget, testing::ScriptedGitClient};

    fn target() -> RebaseTarget {
        RebaseTarget {
            top: ".".into(),
            onto: "main".into(),
            feature: "feat/x".into(),
        }
    }

    #[test]
    fn successful_fast_forward_reports_promoted_commits() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("switched\n"),
            ScriptedGitClient::applied("123456789 feat: add a\nabcdef123 feat: add b\n"),
            ScriptedGitClient::applied("Fast-forward\n"),
        ]);

        let result = execute(ApplyRebase { target: target() }, &git)
            .expect("fast-forward application succeeds");

        assert_eq!(result.status, RebaseStatus::FastForwarded);
        assert_eq!(
            result.detail,
            concat!(
                "switched to 'main' from 'feat/x'\n",
                "fast-forwarded main +2 commits:\n",
                "  123456789  feat: add a\n",
                "  abcdef123  feat: add b",
            )
        );
    }

    #[test]
    fn failed_merge_surfaces_git_error() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("switched\n"),
            ScriptedGitClient::rejected("log unavailable"),
            ScriptedGitClient::rejected("fatal: not ff"),
        ]);

        let result = execute(ApplyRebase { target: target() }, &git)
            .expect("a rejected log and merge remain closed outcomes");

        assert_eq!(result.status, RebaseStatus::Failed);
        assert!(result.detail.contains("not ff"));
        assert_eq!(
            result.progress,
            RebaseProgress {
                branch_switched: true,
                promoted_commits: None,
                fast_forwarded: false,
                recovery: Some(BranchRecovery {
                    original_branch: "feat/x".into(),
                    command: "git switch feat/x".into(),
                }),
            }
        );
    }

    #[test]
    fn promoted_commit_transport_failure_remains_a_sourced_apply_error() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("switched\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(ApplyRebase { target: target() }, &git)
            .expect_err("transport failure must remain an error");

        assert!(error.to_string().starts_with("read promoted commits:"));
        assert!(error.to_string().ends_with(": git transport unavailable"));
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let ApplyRebaseError::Transport { progress, .. } = error;
        assert!(progress.branch_switched);
        assert_eq!(progress.promoted_commits, None);
        assert_eq!(
            progress.recovery,
            Some(BranchRecovery {
                original_branch: "feat/x".into(),
                command: "git switch feat/x".into(),
            })
        );
    }
}
