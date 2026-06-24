use std::path::Path;

use crate::commands::squash_local::{GitOutput, GitRunner};
use crate::git;

/// Status of an applied `sw` flow. Maps to exit codes in `lib.rs` dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Refused,
    Noop,
    Fail,
}

/// Outcome of an applied flow: the status plus a human line (log on success, reason on failure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwResult {
    pub status: Status,
    pub detail: String,
}

impl SwResult {
    fn new(status: Status, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }
}

/// Read-only plan for the switch-only flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchPlan {
    /// Cannot switch; the string explains why (stderr, exit 1).
    Refused(String),
    /// Ready to switch: resolved repo root `top`, current branch `from`, target branch `onto`.
    Ready {
        top: String,
        onto: String,
        from: String,
    },
    /// Already on `onto`; nothing to do (stdout, exit 0).
    AlreadyThere(String),
}

// ? Unlike sync::capture, empty stdout on a clean exit is a valid value (onto_exists checks the exit code only).
/// Runs git and returns trimmed stdout on a clean exit, else `None`.
fn capture(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> Option<String> {
    match runner.run(repo, args) {
        Ok(out) if out.exit_code == 0 => Some(out.stdout.trim().to_string()),
        _ => None,
    }
}

/// True when `<onto>` exists as a local branch.
fn onto_exists(runner: &impl GitRunner, repo: &Path, onto: &str) -> bool {
    matches!(
        runner.run(repo, &["rev-parse", "--verify", &format!("refs/heads/{onto}")]),
        Ok(out) if out.exit_code == 0
    )
}

pub fn plan_switch(runner: &impl GitRunner, repo: &Path, onto: &str) -> SwitchPlan {
    let top_str = match capture(runner, repo, &["rev-parse", "--show-toplevel"]) {
        Some(top) if !top.is_empty() => top,
        _ => return SwitchPlan::Refused("not a git repo".to_string()),
    };
    let top_path = Path::new(&top_str);

    let branch = match capture(runner, top_path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) => branch,
        None => return SwitchPlan::Refused("not a git repo".to_string()),
    };
    if branch == onto {
        return SwitchPlan::AlreadyThere(onto.to_string());
    }
    if !onto_exists(runner, top_path, onto) {
        return SwitchPlan::Refused(format!("no '{onto}' branch (use --onto <branch>)"));
    }
    SwitchPlan::Ready {
        top: top_str,
        onto: onto.to_string(),
        from: branch,
    }
}

pub fn apply_switch(runner: &impl GitRunner, top: &Path, onto: &str, from: &str) -> SwResult {
    match runner.run(top, &["switch", onto]) {
        Ok(out) if out.exit_code == 0 => {
            SwResult::new(Status::Ok, format!("switched to '{onto}' from '{from}'"))
        }
        Ok(out) => SwResult::new(Status::Fail, git_error(&out)),
        Err(error) => SwResult::new(Status::Fail, error.to_string()),
    }
}

/// git's own message ([`GitOutput::diagnostic`]) if it gave one, else a generic line.
/// Shared by every `apply_*` flow (switch, merge, branch), so the fallback names no
/// specific git subcommand.
fn git_error(out: &GitOutput) -> String {
    match out.diagnostic() {
        said if !said.is_empty() => said.to_string(),
        _ => format!("git command failed (exit {})", out.exit_code),
    }
}

/// What a fast-forward will promote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseTarget {
    /// Absolute repo top (from `rev-parse --show-toplevel`); `apply_*` runs git here.
    pub top: String,
    pub onto: String,
    pub feature: String,
}

impl RebaseTarget {
    /// `<onto>..<feature>` — the commits being promoted.
    pub fn range(&self) -> String {
        format!("{}..{}", self.onto, self.feature)
    }
}

/// Read-only plan for the `--rebase` flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebasePlan {
    Refused(String),
    Ready(RebaseTarget),
    /// `<onto>` is already at the feature tip; nothing to promote (stdout, exit 0).
    Noop(String),
}

/// True when `ancestor` is an ancestor of `descendant` (a clean fast-forward is possible).
fn is_ancestor(runner: &impl GitRunner, repo: &Path, ancestor: &str, descendant: &str) -> bool {
    matches!(
        runner.run(repo, &["merge-base", "--is-ancestor", ancestor, descendant]),
        Ok(out) if out.exit_code == 0
    )
}

/// `git rev-list --count <range>` as a usize, or `None` if the command failed (so callers
/// can tell a genuine zero from an error rather than conflating them).
fn count_range(runner: &impl GitRunner, repo: &Path, range: &str) -> Option<usize> {
    capture(runner, repo, &["rev-list", "--count", range]).and_then(|s| s.parse().ok())
}

pub fn plan_rebase(runner: &impl GitRunner, repo: &Path, onto: &str) -> RebasePlan {
    let top = match capture(runner, repo, &["rev-parse", "--show-toplevel"]) {
        Some(top) if !top.is_empty() => top,
        _ => return RebasePlan::Refused("not a git repo".to_string()),
    };
    let top = Path::new(&top);

    let feature = match capture(runner, top, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch != "HEAD" => branch,
        Some(_) => {
            return RebasePlan::Refused("detached HEAD — checkout a branch first".to_string());
        }
        None => return RebasePlan::Refused("not a git repo".to_string()),
    };
    if feature == onto {
        return RebasePlan::Refused(format!("already on '{onto}' — nothing to promote"));
    }
    if !onto_exists(runner, top, onto) {
        return RebasePlan::Refused(format!("no '{onto}' branch (use --onto <branch>)"));
    }
    match capture(runner, top, &["status", "--porcelain"]) {
        Some(s) if !s.is_empty() => {
            return RebasePlan::Refused(
                "working tree not clean — commit or stash first".to_string(),
            );
        }
        None => return RebasePlan::Refused("git status failed".to_string()),
        Some(_) => {}
    }
    if !is_ancestor(runner, top, onto, &feature) {
        // The count is cosmetic here (how many commits onto holds that feature lacks); a
        // failure degrades to 0 rather than blocking the already-correct refusal.
        let extra = count_range(runner, top, &format!("{feature}..{onto}")).unwrap_or(0);
        return RebasePlan::Refused(format!(
            "'{onto}' has diverged from '{feature}' (+{extra} commits it lacks); fast-forward unsafe — rebase or merge manually"
        ));
    }
    let range = format!("{onto}..{feature}");
    match count_range(runner, top, &range) {
        Some(0) => {
            return RebasePlan::Noop(format!("'{onto}' already up to date with '{feature}'"));
        }
        None => return RebasePlan::Refused("git rev-list failed".to_string()),
        Some(_) => {}
    }
    RebasePlan::Ready(RebaseTarget {
        top: top.to_string_lossy().into_owned(),
        onto: onto.to_string(),
        feature,
    })
}

pub fn apply_rebase(runner: &impl GitRunner, target: &RebaseTarget) -> SwResult {
    let top = Path::new(&target.top);
    match runner.run(top, &["switch", &target.onto]) {
        Ok(out) if out.exit_code == 0 => {}
        Ok(out) => return SwResult::new(Status::Fail, git_error(&out)),
        Err(error) => return SwResult::new(Status::Fail, error.to_string()),
    }
    // ! Capture the promoted commits BEFORE the fast-forward — afterwards `onto` points at
    // ! the feature tip, so `onto..feature` is empty and the log would read "+0 commits".
    let commits = git::log_commits(&target.top, &target.range()).unwrap_or_default();
    match runner.run(top, &["merge", "--ff-only", &target.feature]) {
        Ok(out) if out.exit_code == 0 => SwResult::new(Status::Ok, rebase_log(target, &commits)),
        Ok(out) => SwResult::new(Status::Fail, git_error(&out)),
        Err(error) => SwResult::new(Status::Fail, error.to_string()),
    }
}

/// What a revert will reset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevertTarget {
    /// Absolute repo top; `apply_revert` runs git here.
    pub top: String,
    pub onto: String,
    /// `<onto>`'s position before the last fast-forward, resolved to a concrete sha.
    pub prior_sha: String,
}

/// Read-only plan for the `--revert` flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertPlan {
    Refused(String),
    Ready(RevertTarget),
}

pub fn plan_revert(runner: &impl GitRunner, repo: &Path, onto: &str) -> RevertPlan {
    let top_str = match capture(runner, repo, &["rev-parse", "--show-toplevel"]) {
        Some(top) if !top.is_empty() => top,
        _ => return RevertPlan::Refused("not a git repo".to_string()),
    };
    let top = Path::new(&top_str);

    match capture(runner, top, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch == onto => {}
        Some(branch) if branch == "HEAD" => {
            return RevertPlan::Refused("detached HEAD — checkout a branch first".to_string());
        }
        Some(_) => {
            return RevertPlan::Refused(format!(
                "revert expects to be on '{onto}' (the branch the last sw rebased)"
            ));
        }
        None => return RevertPlan::Refused("not a git repo".to_string()),
    }

    // ? Revert force-moves <onto>; a dirty tree would carry uncommitted work onto the branch
    // ? we switch back to, atop a rewound <onto>. Refuse, mirroring the rebase preflight.
    match capture(runner, top, &["status", "--porcelain"]) {
        Some(s) if !s.is_empty() => {
            return RevertPlan::Refused(
                "working tree not clean — commit or stash first".to_string(),
            );
        }
        None => return RevertPlan::Refused("git status failed".to_string()),
        Some(_) => {}
    }

    let prior_ref = format!("{onto}@{{1}}");
    let prior_sha = match capture(runner, top, &["rev-parse", &prior_ref]) {
        Some(sha) if !sha.is_empty() => sha,
        _ => return RevertPlan::Refused(format!("no prior position for '{onto}' in the reflog")),
    };

    // ? Guard on the reflog ref `<onto>@{1}` (not the resolved prior_sha) to match the
    // ? `merge-base --is-ancestor <onto>@{1} <onto>` fast-forward check the spec specifies.
    if !is_ancestor(runner, top, &prior_ref, onto) {
        return RevertPlan::Refused(format!(
            "'{onto}' moved in a way that isn't a simple fast-forward; refusing to auto-revert"
        ));
    }

    RevertPlan::Ready(RevertTarget {
        top: top_str,
        onto: onto.to_string(),
        prior_sha,
    })
}

pub fn apply_revert(runner: &impl GitRunner, target: &RevertTarget) -> SwResult {
    let top = Path::new(&target.top);
    match runner.run(top, &["switch", "-"]) {
        Ok(out) if out.exit_code == 0 => {}
        Ok(out) => return SwResult::new(Status::Fail, git_error(&out)),
        Err(error) => return SwResult::new(Status::Fail, error.to_string()),
    }
    match runner.run(top, &["branch", "-f", &target.onto, &target.prior_sha]) {
        Ok(out) if out.exit_code == 0 => SwResult::new(
            Status::Ok,
            format!(
                "reverted '{}' to {} and switched back",
                target.onto, &target.prior_sha
            ),
        ),
        Ok(out) => SwResult::new(Status::Fail, git_error(&out)),
        Err(error) => SwResult::new(Status::Fail, error.to_string()),
    }
}

/// The multi-line success log: switch line + a `+N commits` list of the promoted commits.
/// `commits` must be captured BEFORE the fast-forward (see [`apply_rebase`]).
fn rebase_log(target: &RebaseTarget, commits: &[crate::model::Commit]) -> String {
    let mut out = format!(
        "switched to '{}' from '{}'\nfast-forwarded {} +{} commits:",
        target.onto,
        target.feature,
        target.onto,
        commits.len()
    );
    for c in commits {
        out.push_str(&format!("\n  {}  {}", c.sha, c.subject));
    }
    out
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;

    use super::*;
    use crate::commands::squash_local::GitOutput;

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
        fn exit(stdout: &str, code: i32) -> GitOutput {
            GitOutput {
                stdout: stdout.into(),
                stderr: String::new(),
                exit_code: code,
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

    #[test]
    fn plan_switch_ready_reports_onto_and_current_branch() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),   // rev-parse --show-toplevel
            FakeRunner::ok("feat/x\n"),          // rev-parse --abbrev-ref HEAD
            FakeRunner::ok("refs/heads/main\n"), // rev-parse --verify refs/heads/main
        ]);
        let plan = plan_switch(&runner, Path::new("."), "main");
        assert_eq!(
            plan,
            SwitchPlan::Ready {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                from: "feat/x".into()
            }
        );
    }

    #[test]
    fn plan_switch_noops_when_already_on_onto() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("main\n"),
        ]);
        let plan = plan_switch(&runner, Path::new("."), "main");
        assert_eq!(plan, SwitchPlan::AlreadyThere("main".into()));
    }

    #[test]
    fn plan_switch_refuses_when_onto_missing() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/x\n"),
            FakeRunner::exit("", 128), // verify refs/heads/main fails
        ]);
        let plan = plan_switch(&runner, Path::new("."), "main");
        assert_eq!(
            plan,
            SwitchPlan::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn apply_switch_runs_git_switch_and_reports() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("")]); // switch main
        let result = apply_switch(&runner, Path::new("."), "main", "feat/x");
        assert_eq!(result.status, Status::Ok);
        assert_eq!(result.detail, "switched to 'main' from 'feat/x'");
        assert_eq!(runner.arg_lists(), vec![vec!["switch", "main"]]);
    }

    #[test]
    fn apply_switch_fails_surfaces_git_error() {
        // git writes its real diagnostic to stderr; git_error must surface it.
        let runner = FakeRunner::new(vec![FakeRunner::exit_err(
            "error: Your local changes would be overwritten",
            1,
        )]);
        let result = apply_switch(&runner, Path::new("."), "main", "feat/x");
        assert_eq!(result.status, Status::Fail);
        assert!(result.detail.contains("local changes would be overwritten"));
    }

    #[test]
    fn git_error_falls_back_to_stdout_then_generic() {
        // stderr empty but stdout has a message -> use stdout.
        assert_eq!(
            git_error(&FakeRunner::exit("some stdout note", 3)),
            "some stdout note"
        );
        // both empty -> generic line with the exit code.
        assert_eq!(
            git_error(&FakeRunner::exit("", 5)),
            "git command failed (exit 5)"
        );
    }

    #[test]
    fn plan_rebase_ready_when_onto_is_ancestor_and_behind() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),   // show-toplevel
            FakeRunner::ok("feat/x\n"),          // abbrev-ref HEAD
            FakeRunner::ok("refs/heads/main\n"), // verify refs/heads/main
            FakeRunner::ok(""),                  // status --porcelain (clean)
            FakeRunner::ok(""), // merge-base --is-ancestor main feat/x (exit 0 = ancestor)
            FakeRunner::ok("3\n"), // rev-list --count main..feat/x (ahead)
        ]);
        let plan = plan_rebase(&runner, Path::new("."), "main");
        assert_eq!(
            plan,
            RebasePlan::Ready(RebaseTarget {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                feature: "feat/x".into(),
            })
        );
    }

    #[test]
    fn plan_rebase_refuses_when_already_on_onto() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("main\n"),
        ]);
        assert_eq!(
            plan_rebase(&runner, Path::new("."), "main"),
            RebasePlan::Refused("already on 'main' — nothing to promote".into())
        );
    }

    #[test]
    fn plan_rebase_refuses_on_dirty_tree() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/x\n"),
            FakeRunner::ok("refs/heads/main\n"),
            FakeRunner::ok(" M a.rs\n"), // dirty
        ]);
        assert_eq!(
            plan_rebase(&runner, Path::new("."), "main"),
            RebasePlan::Refused("working tree not clean — commit or stash first".into())
        );
    }

    #[test]
    fn plan_rebase_refuses_when_onto_missing() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/x\n"),
            FakeRunner::exit("", 128), // verify refs/heads/main fails
        ]);
        assert_eq!(
            plan_rebase(&runner, Path::new("."), "main"),
            RebasePlan::Refused("no 'main' branch (use --onto <branch>)".into())
        );
    }

    #[test]
    fn plan_rebase_refuses_when_diverged() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/x\n"),
            FakeRunner::ok("refs/heads/main\n"),
            FakeRunner::ok(""),      // clean
            FakeRunner::exit("", 1), // is-ancestor: NOT ancestor
            FakeRunner::ok("2\n"),   // rev-list --count feat/x..main (commits onto has)
        ]);
        assert_eq!(
            plan_rebase(&runner, Path::new("."), "main"),
            RebasePlan::Refused(
                "'main' has diverged from 'feat/x' (+2 commits it lacks); fast-forward unsafe — rebase or merge manually".into()
            )
        );
    }

    #[test]
    fn plan_rebase_noops_when_onto_already_at_feature_tip() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/x\n"),
            FakeRunner::ok("refs/heads/main\n"),
            FakeRunner::ok(""),    // clean
            FakeRunner::ok(""),    // is-ancestor: ancestor
            FakeRunner::ok("0\n"), // rev-list --count main..feat/x = 0 (nothing ahead)
        ]);
        assert_eq!(
            plan_rebase(&runner, Path::new("."), "main"),
            RebasePlan::Noop("'main' already up to date with 'feat/x'".into())
        );
    }

    #[test]
    fn plan_rebase_refuses_when_rev_list_count_fails() {
        // A failed `rev-list --count` must not be read as "0 commits / already up to date".
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/x\n"),
            FakeRunner::ok("refs/heads/main\n"),
            FakeRunner::ok(""),          // clean
            FakeRunner::ok(""),          // is-ancestor: ancestor
            FakeRunner::exit_err("", 1), // rev-list --count fails
        ]);
        assert_eq!(
            plan_rebase(&runner, Path::new("."), "main"),
            RebasePlan::Refused("git rev-list failed".into())
        );
    }

    #[test]
    fn apply_rebase_switches_then_ff_merges() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("switched\n"),     // switch main
            FakeRunner::ok("Fast-forward\n"), // merge --ff-only feat/x
        ]);
        let target = RebaseTarget {
            top: ".".into(),
            onto: "main".into(),
            feature: "feat/x".into(),
        };
        let result = apply_rebase(&runner, &target);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(
            runner.arg_lists(),
            vec![vec!["switch", "main"], vec!["merge", "--ff-only", "feat/x"]]
        );
        assert!(
            result
                .detail
                .starts_with("switched to 'main' from 'feat/x'")
        );
    }

    #[test]
    fn apply_rebase_fails_if_merge_fails_after_switch() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("switched\n"),         // switch ok
            FakeRunner::exit("fatal: not ff", 1), // merge fails
        ]);
        let target = RebaseTarget {
            top: ".".into(),
            onto: "main".into(),
            feature: "feat/x".into(),
        };
        let result = apply_rebase(&runner, &target);
        assert_eq!(result.status, Status::Fail);
        assert!(result.detail.contains("not ff"));
    }

    #[test]
    fn plan_revert_ready_when_on_onto_and_last_move_was_ff() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"), // show-toplevel
            FakeRunner::ok("main\n"),          // abbrev-ref HEAD (on onto)
            FakeRunner::ok(""),                // status --porcelain (clean)
            FakeRunner::ok("abc123\n"),        // rev-parse main@{1} (prior sha)
            FakeRunner::ok(""),                // is-ancestor main@{1} main (exit 0)
        ]);
        let plan = plan_revert(&runner, Path::new("."), "main");
        assert_eq!(
            plan,
            RevertPlan::Ready(RevertTarget {
                top: "/home/me/repo".into(),
                onto: "main".into(),
                prior_sha: "abc123".into()
            })
        );
    }

    #[test]
    fn plan_revert_refuses_when_not_on_onto() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("feat/x\n"), // not on main
        ]);
        assert_eq!(
            plan_revert(&runner, Path::new("."), "main"),
            RevertPlan::Refused(
                "revert expects to be on 'main' (the branch the last sw rebased)".into()
            )
        );
    }

    #[test]
    fn plan_revert_refuses_on_detached_head() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("HEAD\n"), // detached
        ]);
        assert_eq!(
            plan_revert(&runner, Path::new("."), "main"),
            RevertPlan::Refused("detached HEAD — checkout a branch first".into())
        );
    }

    #[test]
    fn plan_revert_refuses_on_dirty_tree() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("main\n"),    // on onto
            FakeRunner::ok(" M a.rs\n"), // status --porcelain: dirty
        ]);
        assert_eq!(
            plan_revert(&runner, Path::new("."), "main"),
            RevertPlan::Refused("working tree not clean — commit or stash first".into())
        );
    }

    #[test]
    fn plan_revert_refuses_without_prior_reflog() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("main\n"),
            FakeRunner::ok(""),        // status --porcelain (clean)
            FakeRunner::exit("", 128), // rev-parse main@{1} fails
        ]);
        assert_eq!(
            plan_revert(&runner, Path::new("."), "main"),
            RevertPlan::Refused("no prior position for 'main' in the reflog".into())
        );
    }

    #[test]
    fn plan_revert_refuses_when_last_move_not_fast_forward() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/repo\n"),
            FakeRunner::ok("main\n"),
            FakeRunner::ok(""), // status --porcelain (clean)
            FakeRunner::ok("abc123\n"),
            FakeRunner::exit("", 1), // is-ancestor fails -> not a ff
        ]);
        assert_eq!(
            plan_revert(&runner, Path::new("."), "main"),
            RevertPlan::Refused(
                "'main' moved in a way that isn't a simple fast-forward; refusing to auto-revert"
                    .into()
            )
        );
    }

    #[test]
    fn apply_revert_switches_back_then_resets_onto() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("switched\n"), // switch -
            FakeRunner::ok(""),           // branch -f main abc123
        ]);
        let target = RevertTarget {
            top: ".".into(),
            onto: "main".into(),
            prior_sha: "abc123".into(),
        };
        let result = apply_revert(&runner, &target);
        assert_eq!(result.status, Status::Ok);
        assert_eq!(
            runner.arg_lists(),
            vec![vec!["switch", "-"], vec!["branch", "-f", "main", "abc123"]]
        );
        assert!(result.detail.contains("reverted 'main'"));
    }
}
