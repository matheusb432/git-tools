//! Applies a planned fast-forward and reports the promoted commits.

use std::fmt::Write as _;

use super::{branch_recovery::BranchRecovery, plan_rebase::RebaseTarget};
use crate::{ports::GitRunner, shared::git::command_label};

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
pub struct RebaseResult {
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

impl RebaseResult {
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
    git: &impl GitRunner,
) -> Result<RebaseResult, ApplyRebaseError> {
    let ApplyRebase { target } = command;
    let mut progress = RebaseProgress::default();
    let switch_args = ["switch", target.onto.as_str()];
    match git.run(&target.top, &switch_args) {
        Ok(output) if output.exit_code == 0 => {}
        Ok(output) => {
            return Ok(RebaseResult::new(
                RebaseStatus::Failed,
                output.error_line(),
                progress,
            ));
        }
        Err(source) => return Err(transport(&switch_args, progress, source)),
    }
    progress.branch_switched = true;
    progress.recovery = Some(BranchRecovery::switch_to(&target.feature));

    // The range becomes empty after the fast-forward, so select promoted commits first.
    let commits = promoted_commits(git, &target, &progress)?;
    progress.promoted_commits.clone_from(&commits);
    let merge_args = ["merge", "--ff-only", target.feature.as_str()];
    match git.run(&target.top, &merge_args) {
        Ok(output) if output.exit_code == 0 => {
            progress.fast_forwarded = true;
            progress.recovery = None;
            Ok(RebaseResult::new(
                RebaseStatus::FastForwarded,
                rebase_log(&target, commits.as_deref().unwrap_or_default()),
                progress,
            ))
        }
        Ok(output) => Ok(RebaseResult::new(
            RebaseStatus::Failed,
            output.error_line(),
            progress,
        )),
        Err(source) => Err(transport(&merge_args, progress, source)),
    }
}

fn promoted_commits(
    git: &impl GitRunner,
    target: &RebaseTarget,
    progress: &RebaseProgress,
) -> Result<Option<Vec<PromotedCommit>>, ApplyRebaseError> {
    let range = target.range();
    let args = [
        "log",
        "--date=format:%Y-%m-%d %H:%M",
        "--format=%H%x1f%s%x1f%b%x1f%ad%x1f%aI%x1f%P%x1e",
        &range,
    ];
    match git.run(&target.top, &args) {
        Ok(output) if output.exit_code == 0 => Ok(Some(parse_promoted_commits(&output.stdout))),
        Ok(_) => Ok(None),
        Err(source) => Err(transport(&args, progress.clone(), source)),
    }
}

fn transport(args: &[&str], progress: RebaseProgress, source: anyhow::Error) -> ApplyRebaseError {
    ApplyRebaseError::Transport {
        command: command_label(args),
        progress,
        source,
    }
}

fn parse_promoted_commits(raw: &str) -> Vec<PromotedCommit> {
    raw.split('\x1e')
        .map(str::trim)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let mut fields = record.split('\x1f');
            PromotedCommit {
                sha: fields.next().unwrap_or("").chars().take(9).collect(),
                subject: fields.next().unwrap_or("").to_string(),
            }
        })
        .collect()
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
    use crate::{branches::plan_rebase::RebaseTarget, testing::FakeGitRunner};

    fn target() -> RebaseTarget {
        RebaseTarget {
            top: ".".into(),
            onto: "main".into(),
            feature: "feat/x".into(),
        }
    }

    #[test]
    fn successful_fast_forward_reports_promoted_commits() {
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("switched\n"),
            FakeGitRunner::ok(
                "123456789abcdef\u{1f}feat: add a\u{1f}\u{1f}\u{1f}\u{1f}\u{1e}\n\
                 abcdef123456789\u{1f}feat: add b\u{1f}\u{1f}\u{1f}\u{1f}\u{1e}\n",
            ),
            FakeGitRunner::ok("Fast-forward\n"),
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
        let git = FakeGitRunner::new(vec![
            FakeGitRunner::ok("switched\n"),
            FakeGitRunner::exit_err("log unavailable", 1),
            FakeGitRunner::exit_err("fatal: not ff", 1),
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
        let git = FakeGitRunner::with_results(vec![
            Ok(FakeGitRunner::ok("switched\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let error = execute(ApplyRebase { target: target() }, &git)
            .expect_err("transport failure must remain an error");

        assert!(error.to_string().starts_with("git log "));
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
