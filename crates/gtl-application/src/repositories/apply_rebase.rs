//! Applies a planned fast-forward and reports the promoted commits.

use std::fmt::Write as _;

use gtl_models::diffs::{CommitId, CommitIdAbbreviation};

use super::{BranchRecovery, plan_rebase::RebaseTarget};
use crate::ports::{GitClient, GitEffect};

/// Requests applying one confirmed fast-forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyRebase {
    pub target: RebaseTarget,
}

/// Reports the closed fast-forward status and its user-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyRebaseOk {
    /// The target branch was fast-forwarded with the available commit summary.
    FastForwarded {
        detail: String,
        promoted_commits: PromotedCommits,
    },
    /// Git rejected an application step after the reported partial progress.
    Failed {
        detail: String,
        progress: RebaseProgress,
    },
}

/// Reports the last completed fast-forward step and its recovery data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RebaseProgress {
    /// The operation has not changed the repository.
    #[default]
    NotStarted,
    /// The target branch is checked out and the original branch can be restored.
    BranchSwitched { recovery: BranchRecovery },
    /// Promoted commits were inspected before the pending fast-forward.
    Prepared {
        promoted_commits: PromotedCommits,
        recovery: BranchRecovery,
    },
}

/// Records whether Git could describe the commits promoted by a fast-forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotedCommits {
    /// Git rejected the informational log query, so the commit list is unavailable.
    Unavailable,
    /// Git returned the complete, possibly empty promoted-commit list.
    Known(Vec<PromotedCommit>),
}

impl PromotedCommits {
    fn as_slice(&self) -> &[PromotedCommit] {
        match self {
            Self::Unavailable => &[],
            Self::Known(commits) => commits,
        }
    }
}

/// Identifies one feature commit promoted by the fast-forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotedCommit {
    /// Identifies the promoted commit using its full validated Git commit ID.
    pub id: CommitId,
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
            return Ok(ApplyRebaseOk::Failed { detail, progress });
        }
        Err(source) => return Err(transport("switch branch", progress, source)),
    }
    let recovery = BranchRecovery::switch_to(&target.feature);
    progress = RebaseProgress::BranchSwitched {
        recovery: recovery.clone(),
    };

    // The range becomes empty after the fast-forward, so select promoted commits first.
    let commits = promoted_commits(git, &target, &progress)?;
    progress = RebaseProgress::Prepared {
        promoted_commits: commits.clone(),
        recovery,
    };
    match git.fast_forward(
        &target.top,
        &gtl_models::git::GitRevision::from(&target.feature),
    ) {
        Ok(GitEffect::Applied(_)) => Ok(ApplyRebaseOk::FastForwarded {
            detail: rebase_log(&target, commits.as_slice()),
            promoted_commits: commits,
        }),
        Ok(GitEffect::Rejected(detail)) => Ok(ApplyRebaseOk::Failed { detail, progress }),
        Err(source) => Err(transport("fast-forward branch", progress, source)),
    }
}

fn promoted_commits(
    git: &impl GitClient,
    target: &RebaseTarget,
    progress: &RebaseProgress,
) -> Result<PromotedCommits, ApplyRebaseError> {
    let range = target.range();
    match git
        .brief_log(&target.top, &range)
        .map_err(|source| transport("read promoted commits", progress.clone(), source))?
    {
        GitEffect::Rejected(_) => Ok(PromotedCommits::Unavailable),
        GitEffect::Applied(entries) => Ok(PromotedCommits::Known(
            entries
                .into_iter()
                .map(|entry| PromotedCommit {
                    id: entry.id,
                    subject: entry.subject,
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
        let _ = write!(
            detail,
            "\n  {}  {}",
            commit.id.abbreviated(CommitIdAbbreviation::SevenCharacters),
            commit.subject
        );
    }
    detail
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;
    use crate::{
        repositories::{apply_rebase, plan_rebase::RebaseTarget},
        utils::{ScriptedGitClient, branch_name},
    };

    fn target() -> RebaseTarget {
        RebaseTarget {
            top: crate::utils::repository_root("/repo"),
            onto: branch_name("main"),
            feature: branch_name("feat/x"),
        }
    }

    #[test]
    fn successful_fast_forward_reports_promoted_commits() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("switched\n"),
            ScriptedGitClient::applied(concat!(
                "1234567890123456789012345678901234567890\x1ffeat: add a\n",
                "abcdef1234567890abcdef1234567890abcdef12\x1ffeat: add b\n"
            )),
            ScriptedGitClient::applied("Fast-forward\n"),
        ]);

        let result = apply_rebase::execute(ApplyRebase { target: target() }, &git)
            .expect("fast-forward application succeeds");

        let ApplyRebaseOk::FastForwarded {
            detail,
            promoted_commits,
        } = result
        else {
            panic!("fast-forward must report completion");
        };
        assert_eq!(
            detail,
            concat!(
                "switched to 'main' from 'feat/x'\n",
                "fast-forwarded main +2 commits:\n",
                "  1234567  feat: add a\n",
                "  abcdef1  feat: add b",
            )
        );
        assert!(matches!(promoted_commits, PromotedCommits::Known(commits) if commits.len() == 2));
    }

    #[test]
    fn failed_merge_surfaces_git_error() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("switched\n"),
            ScriptedGitClient::rejected("log unavailable"),
            ScriptedGitClient::rejected("fatal: not ff"),
        ]);

        let result = apply_rebase::execute(ApplyRebase { target: target() }, &git)
            .expect("a rejected log and merge remain closed outcomes");

        let ApplyRebaseOk::Failed { detail, progress } = result else {
            panic!("rejected fast-forward must report failure");
        };
        assert!(detail.contains("not ff"));
        assert_eq!(
            progress,
            RebaseProgress::Prepared {
                promoted_commits: PromotedCommits::Unavailable,
                recovery: BranchRecovery {
                    original_branch: branch_name("feat/x"),
                    command: "git switch feat/x".into(),
                },
            }
        );
    }

    #[test]
    fn promoted_commit_transport_failure_remains_a_sourced_apply_error() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("switched\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = apply_rebase::execute(ApplyRebase { target: target() }, &git)
            .expect_err("transport failure must remain an error");

        assert!(error.to_string().starts_with("read promoted commits:"));
        assert!(error.to_string().ends_with(": git transport unavailable"));
        assert_eq!(
            error.source().map(ToString::to_string),
            Some("git transport unavailable".into())
        );
        let ApplyRebaseError::Transport {
            command, progress, ..
        } = error;
        assert_eq!(command, "read promoted commits");
        assert_eq!(
            progress,
            RebaseProgress::BranchSwitched {
                recovery: BranchRecovery {
                    original_branch: branch_name("feat/x"),
                    command: "git switch feat/x".into(),
                },
            }
        );
    }
}
