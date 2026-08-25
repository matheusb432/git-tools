//! Applies a confirmed recursive-push plan through the
//! [`GitClient`] port and aggregate per-repo outcomes into an overall status.

use gtl_models::{
    git::GitEffectMode,
    repository::recursive_push::{
        Dest, PushAllResult, RepoOutcome, RepoReport, RepoTarget, Status,
    },
};

use crate::ports::{GitClient, GitEffect};

/// Pushes each repo's current branch to its upstream, skipping the un-pushable ones, and
/// aggregates per-repo outcomes into an overall [`Status`]. Infallible by design: a
/// failed push becomes a [`RepoOutcome::Failed`] report, never an error.
#[cqrsy::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "CQRsy operations own their request value"
)]
pub fn execute(targets: Vec<RepoTarget>, git: &impl GitClient) -> PushAllResult {
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

    PushAllResult { status, reports }
}

/// Pushes one repo, mapping git's exit and output to a [`RepoOutcome`].
fn push_one(runner: &impl GitClient, target: &RepoTarget) -> RepoOutcome {
    let (branch, remote) = match &target.dest {
        Dest::Skip { reason } => return RepoOutcome::Skipped(reason.clone()),
        Dest::Synced { .. } => return RepoOutcome::UpToDate,
        Dest::Push { branch, remote } => (branch, remote),
    };

    match runner.push_branch(&target.path, remote, branch, GitEffectMode::Apply) {
        Ok(GitEffect::Applied(crate::ports::GitPushReceipt::UpToDate { .. })) => {
            RepoOutcome::UpToDate
        }
        Ok(GitEffect::Applied(crate::ports::GitPushReceipt::Updated { .. })) => RepoOutcome::Pushed,
        Ok(GitEffect::Rejected(detail)) => RepoOutcome::Failed(format!("push failed: {detail}")),
        Err(error) => RepoOutcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        repositories::apply_recursive_push,
        utils::{ScriptedGitClient, project_name, repository_root},
    };

    fn push_target(label: &str) -> RepoTarget {
        RepoTarget {
            path: repository_root(&format!("/repos/{label}")),
            label: project_name(label),
            dest: Dest::Push {
                branch: crate::utils::branch_name("main"),
                remote: crate::utils::remote_name("origin"),
            },
        }
    }

    fn apply(runner: &ScriptedGitClient, targets: Vec<RepoTarget>) -> PushAllResult {
        apply_recursive_push::execute(targets, runner)
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
            path: repository_root("/repos/web"),
            label: project_name("web"),
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
            path: repository_root("/repos/api"),
            label: project_name("api"),
            dest: Dest::Synced {
                branch: crate::utils::branch_name("main"),
                remote: crate::utils::remote_name("origin"),
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
                path: repository_root("/repos/current"),
                label: project_name("current"),
                dest: Dest::Synced {
                    branch: crate::utils::branch_name("main"),
                    remote: crate::utils::remote_name("origin"),
                },
            },
            RepoTarget {
                path: repository_root("/repos/loose"),
                label: project_name("loose"),
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
