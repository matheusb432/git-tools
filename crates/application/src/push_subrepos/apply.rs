//! The `push_subrepos/apply` command: push a confirmed plan's targets through the
//! [`GitRunner`] port and aggregate per-repo outcomes into an overall status.

use domain::managed::push_subrepos::{
    Dest, PushAllResult, RepoOutcome, RepoReport, RepoTarget, Status,
};

use crate::{
    ports::{GitOutput, GitRunner},
    shared::push_summary::{PushOutcome, PushSummary},
};

/// Push every pushable target of a confirmed recursive-push plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPush {
    pub targets: Vec<RepoTarget>,
}

/// Pushes each repo's current branch to its upstream, skipping the un-pushable ones, and
/// aggregates per-repo outcomes into an overall [`Status`]. Infallible by design: a
/// failed push becomes a [`RepoOutcome::Failed`] report, never an error.
#[cqrsy::handler(command)]
pub fn execute(command: ApplyPush, git: &impl GitRunner) -> PushAllResult {
    let ApplyPush { targets } = command;
    let reports: Vec<RepoReport> = targets
        .iter()
        .map(|target| RepoReport {
            label: target.label.clone(),
            outcome: push_one(git, target),
        })
        .collect();

    let summary = PushSummary::from_outcomes(
        reports.iter().map(|report| match &report.outcome {
            RepoOutcome::Pushed => PushOutcome::Pushed,
            RepoOutcome::UpToDate | RepoOutcome::Skipped(_) => PushOutcome::Skipped,
            RepoOutcome::Failed(_) => PushOutcome::Failed,
        }),
        false,
    );

    let status = if summary.failed() == 0 {
        Status::Ok
    } else if summary.pushed() == 0 {
        Status::Fail
    } else {
        Status::Partial
    };

    let mut detail = summary.render(exit_code(status));
    for report in &reports {
        let line = match &report.outcome {
            RepoOutcome::Pushed => format!("\n  {}: pushed", report.label),
            RepoOutcome::UpToDate => format!("\n  {}: already up to date", report.label),
            RepoOutcome::Skipped(reason) => format!("\n  {}: skipped — {reason}", report.label),
            RepoOutcome::Failed(reason) => format!("\n  {}: failed — {reason}", report.label),
        };
        detail.push_str(&line);
    }

    PushAllResult {
        status,
        detail,
        reports,
    }
}

/// The exit code the CLI maps a status to; baked into the detail header so the
/// rendered summary matches the process exit.
const fn exit_code(status: Status) -> i32 {
    match status {
        Status::Ok => 0,
        Status::Partial | Status::Fail => 1,
    }
}

/// Pushes one repo, mapping git's exit and output to a [`RepoOutcome`].
fn push_one(runner: &impl GitRunner, target: &RepoTarget) -> RepoOutcome {
    let (branch, remote) = match &target.dest {
        Dest::Skip { reason } => return RepoOutcome::Skipped(reason.clone()),
        Dest::Synced { .. } => return RepoOutcome::UpToDate,
        Dest::Push { branch, remote } => (branch, remote),
    };

    match runner.run(&target.path, &["push", remote, branch]) {
        Ok(out) if out.exit_code == 0 => {
            if is_up_to_date(&out) {
                RepoOutcome::UpToDate
            } else {
                RepoOutcome::Pushed
            }
        }
        Ok(out) => RepoOutcome::Failed(out.fail_detail("push failed")),
        Err(error) => RepoOutcome::Failed(error.to_string()),
    }
}

/// Whether git reported the remote was already current (`Everything up-to-date`).
fn is_up_to_date(out: &GitOutput) -> bool {
    let said = format!("{} {}", out.stdout, out.stderr).to_ascii_lowercase();
    said.contains("up-to-date") || said.contains("up to date")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::testing::FakeGitRunner;

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

    fn apply(runner: &FakeGitRunner, targets: Vec<RepoTarget>) -> PushAllResult {
        execute(ApplyPush { targets }, runner)
    }

    #[test]
    fn apply_pushes_each_pushable_repo() {
        let runner = FakeGitRunner::new(vec![FakeGitRunner::ok(""), FakeGitRunner::ok("")]);
        let result = apply(&runner, vec![push_target("api"), push_target("web")]);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["push", "origin", "main"],
                vec!["push", "origin", "main"],
            ]
        );
        assert!(
            result
                .reports
                .iter()
                .all(|r| r.outcome == RepoOutcome::Pushed)
        );
    }

    #[test]
    fn apply_labels_already_current_remote_as_up_to_date() {
        let runner = FakeGitRunner::new(vec![FakeGitRunner::ok_stderr("Everything up-to-date\n")]);
        let result = apply(&runner, vec![push_target("api")]);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.reports[0].outcome, RepoOutcome::UpToDate);
        assert!(
            result.detail.contains("api: already up to date"),
            "{}",
            result.detail
        );
    }

    #[test]
    fn apply_skips_targets_without_a_destination_without_calling_git() {
        let runner = FakeGitRunner::default();
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
        assert!(
            runner.arg_lists().is_empty(),
            "skipped repos never call git"
        );
    }

    #[test]
    fn apply_reports_synced_targets_as_up_to_date_without_calling_git() {
        let runner = FakeGitRunner::default();
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
        assert!(
            runner.arg_lists().is_empty(),
            "already-synced repos never call git"
        );
    }

    #[test]
    fn apply_counts_synced_and_unpushable_repos_as_skipped() {
        let runner = FakeGitRunner::new(vec![FakeGitRunner::ok("")]);
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

        assert!(
            result
                .detail
                .starts_with("exit 0  -  3 repos: 1 pushed, 2 skipped\n  pushed: pushed")
        );
    }

    #[test]
    fn apply_reports_partial_when_one_push_fails() {
        let runner = FakeGitRunner::new(vec![
            FakeGitRunner::ok(""),
            FakeGitRunner::exit_err("fatal: remote rejected", 1),
        ]);
        let result = apply(&runner, vec![push_target("api"), push_target("web")]);
        assert_eq!(result.status, Status::Partial);
        assert_eq!(result.reports[0].outcome, RepoOutcome::Pushed);
        assert!(matches!(result.reports[1].outcome, RepoOutcome::Failed(_)));
        assert!(
            result
                .detail
                .starts_with("exit 1  -  2 repos: 1 pushed, 0 skipped, 1 fail")
        );
        assert!(!result.detail.contains("0 warn"));
        assert!(result.detail.contains("web: failed —"), "{}", result.detail);
    }

    #[test]
    fn apply_reports_fail_when_every_push_fails() {
        let runner = FakeGitRunner::new(vec![FakeGitRunner::exit_err("nope", 1)]);
        let result = apply(&runner, vec![push_target("api")]);
        assert_eq!(result.status, Status::Fail);
    }
}
