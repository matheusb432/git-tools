//! The `push_subrepos/apply` command: push a confirmed plan's targets through the
//! [`GitClient`] port and aggregate per-repo outcomes into an overall status.

use gtl_models::managed::push_subrepos::{
    Dest, PushAllResult, RepoOutcome, RepoReport, RepoTarget, Status,
};

use crate::ports::{GitClient, GitEffect};

/// Push every pushable target of a confirmed recursive-push plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPush {
    pub targets: Vec<RepoTarget>,
}

pub type ApplyPushOk = PushAllResult;

/// Pushes each repo's current branch to its upstream, skipping the un-pushable ones, and
/// aggregates per-repo outcomes into an overall [`Status`]. Infallible by design: a
/// failed push becomes a [`RepoOutcome::Failed`] report, never an error.
#[cqrsy::command]
pub fn execute(command: ApplyPush, git: &impl GitClient) -> ApplyPushOk {
    let ApplyPush { targets } = command;
    let reports: Vec<RepoReport> = targets
        .iter()
        .map(|target| RepoReport {
            label: target.label.clone(),
            outcome: push_one(git, target),
        })
        .collect();

    let failed = reports
        .iter()
        .filter(|report| matches!(report.outcome, RepoOutcome::Failed(_)))
        .count();
    let pushed = reports
        .iter()
        .filter(|report| report.outcome == RepoOutcome::Pushed)
        .count();
    let status = if failed == 0 {
        Status::Ok
    } else if pushed == 0 {
        Status::Fail
    } else {
        Status::Partial
    };

    ApplyPushOk { status, reports }
}

/// Pushes one repo, mapping git's exit and output to a [`RepoOutcome`].
fn push_one(runner: &impl GitClient, target: &RepoTarget) -> RepoOutcome {
    let (branch, remote) = match &target.dest {
        Dest::Skip { reason } => return RepoOutcome::Skipped(reason.clone()),
        Dest::Synced { .. } => return RepoOutcome::UpToDate,
        Dest::Push { branch, remote } => (branch, remote),
    };

    match runner.push_branch(&target.path, remote, branch, false) {
        Ok(GitEffect::Applied(receipt)) => {
            if receipt.up_to_date {
                RepoOutcome::UpToDate
            } else {
                RepoOutcome::Pushed
            }
        }
        Ok(GitEffect::Rejected(detail)) => RepoOutcome::Failed(format!("push failed: {detail}")),
        Err(error) => RepoOutcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::testing::ScriptedGitClient;

    fn push_target(label: &str) -> RepoTarget {
        RepoTarget {
            path: PathBuf::from(format!("/repos/{label}")),
            label: label.to_string(),
            dest: Dest::Push {
                branch: "main".into(),
                remote: "origin".into(),
            },
        }
    }

    fn apply(runner: &ScriptedGitClient, targets: Vec<RepoTarget>) -> ApplyPushOk {
        execute(ApplyPush { targets }, runner)
    }

    #[test]
    fn apply_pushes_each_pushable_repo() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::applied(""),
        ]);
        let result = apply(&runner, vec![push_target("api"), push_target("web")]);
        assert_eq!(result.status, Status::Ok);
        assert!(
            result
                .reports
                .iter()
                .all(|r| r.outcome == RepoOutcome::Pushed)
        );
    }

    #[test]
    fn apply_labels_already_current_remote_as_up_to_date() {
        let runner =
            ScriptedGitClient::new(vec![ScriptedGitClient::applied("Everything up-to-date\n")]);
        let result = apply(&runner, vec![push_target("api")]);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.reports[0].outcome, RepoOutcome::UpToDate);
    }

    #[test]
    fn apply_skips_targets_without_a_destination_without_calling_git() {
        let runner = ScriptedGitClient::default();
        let targets = vec![RepoTarget {
            path: PathBuf::from("/repos/web"),
            label: "web".into(),
            dest: Dest::Skip {
                reason: "detached HEAD".into(),
            },
        }];
        let result = apply(&runner, targets);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(
            result.reports[0].outcome,
            RepoOutcome::Skipped("detached HEAD".into())
        );
    }

    #[test]
    fn apply_reports_synced_targets_as_up_to_date_without_calling_git() {
        let runner = ScriptedGitClient::default();
        let targets = vec![RepoTarget {
            path: PathBuf::from("/repos/api"),
            label: "api".into(),
            dest: Dest::Synced {
                branch: "main".into(),
                remote: "origin".into(),
            },
        }];
        let result = apply(&runner, targets);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.reports[0].outcome, RepoOutcome::UpToDate);
    }

    #[test]
    fn apply_counts_synced_and_unpushable_repos_as_skipped() {
        let runner = ScriptedGitClient::new(vec![ScriptedGitClient::applied("")]);
        let targets = vec![
            push_target("pushed"),
            RepoTarget {
                path: PathBuf::from("/repos/current"),
                label: "current".into(),
                dest: Dest::Synced {
                    branch: "main".into(),
                    remote: "origin".into(),
                },
            },
            RepoTarget {
                path: PathBuf::from("/repos/loose"),
                label: "loose".into(),
                dest: Dest::Skip {
                    reason: "no upstream tracking branch".into(),
                },
            },
        ];

        let result = apply(&runner, targets);

        assert_eq!(result.reports.len(), 3);
    }

    #[test]
    fn apply_reports_partial_when_one_push_fails() {
        let runner = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied(""),
            ScriptedGitClient::rejected("fatal: remote rejected"),
        ]);
        let result = apply(&runner, vec![push_target("api"), push_target("web")]);
        assert_eq!(result.status, Status::Partial);
        assert_eq!(result.reports[0].outcome, RepoOutcome::Pushed);
        assert!(matches!(result.reports[1].outcome, RepoOutcome::Failed(_)));
        assert_eq!(result.reports.len(), 2);
    }

    #[test]
    fn apply_reports_fail_when_every_push_fails() {
        let runner = ScriptedGitClient::new(vec![ScriptedGitClient::rejected("nope")]);
        let result = apply(&runner, vec![push_target("api")]);
        assert_eq!(result.status, Status::Fail);
    }
}
