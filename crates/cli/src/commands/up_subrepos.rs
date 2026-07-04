//! `up subrepos` — push every git repo under the current directory to its upstream.
//!
//! The sibling of [`crate::commands::diff_subrepos`] for pushing: discover every repo under
//! a root (reusing [`crate::commands::discover`]), resolve each one's push destination from
//! *local* refs only (no fetch), confirm, then push. A repo with no upstream — or a
//! detached HEAD — carries a [`Dest::Skip`] reason instead of a push target, so an
//! un-pushable repo is unrepresentable as a push and is reported, never silently dropped.
//! A branch already synced with its upstream (the local `@{u}..HEAD` count is `0`, the same
//! check `gtl ls` reports) becomes a [`Dest::Synced`], so the flow scales with repos that
//! actually have unpushed commits — synced repos never reach the network. Plan/apply split
//! mirrors [`crate::commands::prune`].

use std::path::{Path, PathBuf};

use crate::commands::{
    discover::{discover_git_repos, repo_label},
    squash_local::{GitOutput, GitRunner},
};

/// Where a discovered repo's current branch would be pushed, resolved from local refs.
/// A concrete branch→remote push, an already-synced branch with nothing to push, or a
/// reason the repo is skipped — exactly one, never a combination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    /// The current branch has unpushed commits and the remote its upstream tracks.
    Push { branch: String, remote: String },
    /// The branch has an upstream but no unpushed commits (`@{u}..HEAD` is empty), so a
    /// push would be a network no-op — the same "already synced" state `gtl ls` reports.
    Synced { branch: String, remote: String },
    /// The repo cannot be pushed (no upstream, or detached HEAD); the string says why.
    Skip { reason: String },
}

/// One discovered repo: its path, a display label, and its resolved push destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoTarget {
    pub path: PathBuf,
    pub label: String,
    pub dest: Dest,
}

/// Read-only plan for the `up subrepos` flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubreposPlan {
    /// Nothing to do; the string explains why (stderr, exit 1).
    Refused(String),
    /// Discovered repos with their resolved destinations.
    Ready(Vec<RepoTarget>),
}

/// What happened to one repo during [`apply`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoOutcome {
    /// Pushed new commits.
    Pushed,
    /// Push ran but the remote was already current.
    UpToDate,
    /// Not pushed; the string says why (no upstream, detached HEAD).
    Skipped(String),
    /// Push failed; the string holds git's reason.
    Failed(String),
}

/// One repo's reported outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoReport {
    pub label: String,
    pub outcome: RepoOutcome,
}

/// Overall outcome of an applied `up subrepos`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No repo failed (some may have been skipped or already current).
    Ok,
    /// Some repos pushed, some failed.
    Partial,
    /// Every attempted push failed.
    Fail,
}

/// Applied result: a status, a human-readable detail block, and the per-repo reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushAllResult {
    pub status: Status,
    pub detail: String,
    pub reports: Vec<RepoReport>,
}

/// Runs git and returns trimmed stdout on a clean, non-empty exit, else `None`.
fn capture(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> Option<String> {
    match runner.run(repo, args) {
        Ok(out) if out.exit_code == 0 && !out.stdout.trim().is_empty() => {
            Some(out.stdout.trim().to_string())
        }
        _ => None,
    }
}

/// Resolves one repo's push destination from local refs only — never fetches. A detached
/// HEAD or a branch with no `branch.<name>.remote` becomes a [`Dest::Skip`]; a branch with
/// no unpushed commits becomes a [`Dest::Synced`] so the push is skipped entirely.
pub fn inspect(runner: &impl GitRunner, path: &Path, label: String) -> RepoTarget {
    let dest = match capture(runner, path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch != "HEAD" => {
            match capture(
                runner,
                path,
                &["config", &format!("branch.{branch}.remote")],
            ) {
                Some(remote) if is_synced(runner, path) => Dest::Synced { branch, remote },
                Some(remote) => Dest::Push { branch, remote },
                None => Dest::Skip {
                    reason: "no upstream tracking branch".to_string(),
                },
            }
        }
        _ => Dest::Skip {
            reason: "detached HEAD".to_string(),
        },
    };
    RepoTarget {
        path: path.to_path_buf(),
        label,
        dest,
    }
}

/// Whether the current branch has no unpushed commits — the local `@{u}..HEAD` count is
/// exactly `0`. This mirrors the ahead-count `gtl ls`/`status` and the sibling `diff
/// subrepos` use to spot synced repos, and stays purely local (no fetch). An unavailable
/// count (e.g. no `@{u}` merge ref) is never read as synced — we fall back to pushing.
fn is_synced(runner: &impl GitRunner, path: &Path) -> bool {
    capture(runner, path, &["rev-list", "--count", "@{u}..HEAD"])
        .and_then(|count| count.parse::<usize>().ok())
        == Some(0)
}

/// Discovers every git repo under `root` and resolves each one's push destination.
/// Linked worktrees are skipped (see [`discover_git_repos`]).
pub fn plan(runner: &impl GitRunner, root: &Path) -> anyhow::Result<SubreposPlan> {
    let repos = discover_git_repos(root, false)?;
    if repos.is_empty() {
        return Ok(SubreposPlan::Refused(format!(
            "no git repos found under {}",
            root.display()
        )));
    }
    let targets = repos
        .iter()
        .map(|repo| {
            let label = repo_label(root, repo);
            inspect(runner, repo, label)
        })
        .collect();
    Ok(SubreposPlan::Ready(targets))
}

/// Builds the review block printed before any push. Lists every discovered repo and where
/// its current branch would land — or why it will be skipped — so the user confirms the
/// exact set, not just a count.
pub fn confirmation(root: &Path, targets: &[RepoTarget]) -> String {
    let mut out = format!(
        "up subrepos — push {} repo(s) under {}:",
        targets.len(),
        root.display()
    );
    for target in targets {
        match &target.dest {
            Dest::Push { branch, remote } => {
                out.push_str(&format!("\n  {}  ({branch} → {remote})", target.label));
            }
            Dest::Synced { branch, remote } => {
                out.push_str(&format!(
                    "\n  {}  ({branch} → {remote}, already synced)",
                    target.label
                ));
            }
            Dest::Skip { reason } => {
                out.push_str(&format!("\n  {}  (skip — {reason})", target.label));
            }
        }
    }
    out
}

/// Pushes each repo's current branch to its upstream, skipping the un-pushable ones, and
/// aggregates per-repo outcomes into an overall [`Status`].
pub fn apply(runner: &impl GitRunner, targets: &[RepoTarget]) -> PushAllResult {
    let reports: Vec<RepoReport> = targets
        .iter()
        .map(|target| RepoReport {
            label: target.label.clone(),
            outcome: push_one(runner, target),
        })
        .collect();

    let pushed = reports
        .iter()
        .filter(|report| matches!(report.outcome, RepoOutcome::Pushed | RepoOutcome::UpToDate))
        .count();
    let failed = reports
        .iter()
        .filter(|report| matches!(report.outcome, RepoOutcome::Failed(_)))
        .count();

    let status = if failed == 0 {
        Status::Ok
    } else if pushed == 0 {
        Status::Fail
    } else {
        Status::Partial
    };

    let mut detail = format!("pushed {pushed} repo(s).");
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
    use std::{cell::RefCell, path::PathBuf};

    use super::*;

    struct FakeRunner {
        calls: RefCell<Vec<Vec<String>>>,
        results: RefCell<Vec<GitOutput>>,
    }
    impl FakeRunner {
        fn new(results: Vec<GitOutput>) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                results: RefCell::new(results),
            }
        }
        fn ok(stdout: &str) -> GitOutput {
            GitOutput {
                stdout: stdout.into(),
                stderr: String::new(),
                exit_code: 0,
            }
        }
        fn ok_err(stderr: &str) -> GitOutput {
            GitOutput {
                stdout: String::new(),
                stderr: stderr.into(),
                exit_code: 0,
            }
        }
        fn exit_err(stderr: &str, code: i32) -> GitOutput {
            GitOutput {
                stdout: String::new(),
                stderr: stderr.into(),
                exit_code: code,
            }
        }
        fn arg_lists(&self) -> Vec<Vec<String>> {
            self.calls.borrow().clone()
        }
    }
    impl GitRunner for FakeRunner {
        fn run(&self, _repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
            self.calls
                .borrow_mut()
                .push(args.iter().map(|a| a.to_string()).collect());
            Ok(self.results.borrow_mut().remove(0))
        }
    }

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

    // --- inspect ------------------------------------------------------------

    #[test]
    fn inspect_resolves_push_when_branch_is_ahead() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("main\n"),
            FakeRunner::ok("origin\n"),
            FakeRunner::ok("2\n"), // rev-list --count @{u}..HEAD — two unpushed commits
        ]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Push {
                branch: "main".into(),
                remote: "origin".into(),
            }
        );
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["rev-parse", "--abbrev-ref", "HEAD"],
                vec!["config", "branch.main.remote"],
                vec!["rev-list", "--count", "@{u}..HEAD"],
            ]
        );
    }

    #[test]
    fn inspect_marks_synced_when_no_unpushed_commits() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("main\n"),
            FakeRunner::ok("origin\n"),
            FakeRunner::ok("0\n"), // rev-list --count @{u}..HEAD — nothing to push
        ]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Synced {
                branch: "main".into(),
                remote: "origin".into(),
            }
        );
    }

    #[test]
    fn inspect_pushes_when_ahead_count_is_unavailable() {
        // A failed `rev-list --count` must never read as "0 / already synced" — fall back to
        // attempting the push, exactly as the sibling checks do (sw, diff subrepos).
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("main\n"),
            FakeRunner::ok("origin\n"),
            FakeRunner::exit_err("", 1),
        ]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Push {
                branch: "main".into(),
                remote: "origin".into(),
            }
        );
    }

    #[test]
    fn inspect_skips_branch_without_upstream() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("feat\n"), FakeRunner::exit_err("", 1)]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Skip {
                reason: "no upstream tracking branch".into(),
            }
        );
    }

    #[test]
    fn inspect_skips_detached_head() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("HEAD\n")]);
        let target = inspect(&runner, Path::new("/repos/api"), "api".into());
        assert_eq!(
            target.dest,
            Dest::Skip {
                reason: "detached HEAD".into(),
            }
        );
    }

    // --- confirmation -------------------------------------------------------

    #[test]
    fn confirmation_lists_push_destinations_and_skips() {
        let targets = vec![
            push_target("api"),
            RepoTarget {
                path: PathBuf::from("/repos/web"),
                label: "web".into(),
                dest: Dest::Skip {
                    reason: "no upstream tracking branch".into(),
                },
            },
        ];
        let text = confirmation(Path::new("/repos"), &targets);
        assert!(text.contains("push 2 repo(s) under /repos"), "{text}");
        assert!(text.contains("api  (main → origin)"), "{text}");
        assert!(
            text.contains("web  (skip — no upstream tracking branch)"),
            "{text}"
        );
    }

    #[test]
    fn confirmation_marks_already_synced_repos() {
        let targets = vec![RepoTarget {
            path: PathBuf::from("/repos/api"),
            label: "api".into(),
            dest: Dest::Synced {
                branch: "main".into(),
                remote: "origin".into(),
            },
        }];
        let text = confirmation(Path::new("/repos"), &targets);
        assert!(
            text.contains("api  (main → origin, already synced)"),
            "{text}"
        );
    }

    // --- apply --------------------------------------------------------------

    #[test]
    fn apply_pushes_each_pushable_repo() {
        let runner = FakeRunner::new(vec![FakeRunner::ok(""), FakeRunner::ok("")]);
        let targets = vec![push_target("api"), push_target("web")];
        let result = apply(&runner, &targets);
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
        let runner = FakeRunner::new(vec![FakeRunner::ok_err("Everything up-to-date\n")]);
        let result = apply(&runner, &[push_target("api")]);
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
        let runner = FakeRunner::new(vec![]);
        let targets = vec![RepoTarget {
            path: PathBuf::from("/repos/web"),
            label: "web".into(),
            dest: Dest::Skip {
                reason: "detached HEAD".into(),
            },
        }];
        let result = apply(&runner, &targets);
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
        let runner = FakeRunner::new(vec![]);
        let targets = vec![RepoTarget {
            path: PathBuf::from("/repos/api"),
            label: "api".into(),
            dest: Dest::Synced {
                branch: "main".into(),
                remote: "origin".into(),
            },
        }];
        let result = apply(&runner, &targets);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.reports[0].outcome, RepoOutcome::UpToDate);
        assert!(
            runner.arg_lists().is_empty(),
            "already-synced repos never call git"
        );
    }

    #[test]
    fn apply_reports_partial_when_one_push_fails() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""),
            FakeRunner::exit_err("fatal: remote rejected", 1),
        ]);
        let targets = vec![push_target("api"), push_target("web")];
        let result = apply(&runner, &targets);
        assert_eq!(result.status, Status::Partial);
        assert_eq!(result.reports[0].outcome, RepoOutcome::Pushed);
        assert!(matches!(result.reports[1].outcome, RepoOutcome::Failed(_)));
        assert!(result.detail.contains("web: failed —"), "{}", result.detail);
    }

    #[test]
    fn apply_reports_fail_when_every_push_fails() {
        let runner = FakeRunner::new(vec![FakeRunner::exit_err("nope", 1)]);
        let result = apply(&runner, &[push_target("api")]);
        assert_eq!(result.status, Status::Fail);
    }
}
