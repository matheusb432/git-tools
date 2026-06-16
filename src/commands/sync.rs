use std::path::Path;

use crate::commands::squash_local::GitRunner;

/// What a `sync` will push, gathered read-only for the confirmation prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTarget {
    pub name: String,
    pub top: String,
    pub branch: String,
    pub remote: String,
    pub remote_url: String,
}

/// Outcome of the read-only planning pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Cannot sync; the string explains why (printed to stderr, exit 1).
    Refused(String),
    /// Ready to confirm and apply.
    Ready(SyncTarget),
}

/// Whether the push needs an interactive confirmation, given the `--yes` flag and TTY state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// `--yes` was passed: push without prompting.
    Proceed,
    /// Interactive shell: prompt before pushing.
    Confirm,
    /// Non-interactive shell without `--yes`: refuse rather than auto-push.
    RefuseNonInteractive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Refused,
    Noop,
    Synced,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncResult {
    pub status: Status,
    pub detail: String,
}

impl SyncResult {
    fn new(status: Status, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }
}

pub fn gate(yes: bool, interactive: bool) -> Gate {
    if yes {
        Gate::Proceed
    } else if interactive {
        Gate::Confirm
    } else {
        Gate::RefuseNonInteractive
    }
}

pub fn confirmation(target: &SyncTarget) -> String {
    let remote = if target.remote_url.is_empty() {
        target.remote.clone()
    } else {
        format!("{} ({})", target.remote, target.remote_url)
    };
    format!(
        "sync — review before pushing:\n  repo:   {} ({})\n  branch: {}\n  remote: {}",
        target.name, target.top, target.branch, remote
    )
}

pub fn plan(runner: &impl GitRunner, repo: &Path) -> Plan {
    let top = match capture(runner, repo, &["rev-parse", "--show-toplevel"]) {
        Some(top) => top,
        None => return Plan::Refused("not a git repo".to_string()),
    };
    let top_path = Path::new(&top);

    let branch = match capture(runner, top_path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch != "HEAD" => branch,
        _ => return Plan::Refused("detached HEAD — checkout a branch first".to_string()),
    };

    let remote = match capture(
        runner,
        top_path,
        &["config", &format!("branch.{branch}.remote")],
    ) {
        Some(remote) => remote,
        None => {
            return Plan::Refused(format!(
                "no upstream tracking branch (run: git push -u origin {branch})"
            ));
        }
    };

    // ? remote url is for the prompt only; an unset url must not block the sync.
    let remote_url = capture(runner, top_path, &["remote", "get-url", &remote]).unwrap_or_default();

    Plan::Ready(SyncTarget {
        name: repo_name(&top),
        top,
        branch,
        remote,
        remote_url,
    })
}

pub fn apply(runner: &impl GitRunner, target: &SyncTarget, message: &str) -> SyncResult {
    let top = Path::new(&target.top);

    let dirty = match runner.run(top, &["status", "--porcelain"]) {
        Ok(output) if output.exit_code == 0 => !output.stdout.trim().is_empty(),
        _ => return SyncResult::new(Status::Fail, "git status failed"),
    };

    if !dirty {
        let ahead = match capture(runner, top, &["rev-list", "--count", "@{u}..HEAD"]) {
            Some(count) => count.parse::<usize>().unwrap_or(0),
            None => return SyncResult::new(Status::Fail, "rev-list failed"),
        };
        if ahead == 0 {
            return SyncResult::new(Status::Noop, "nothing to commit; already up to date");
        }
        return match push(runner, top, target) {
            Ok(()) => SyncResult::new(
                Status::Synced,
                format!("nothing to commit; pushed {ahead} commit(s)"),
            ),
            Err(detail) => SyncResult::new(Status::Fail, detail),
        };
    }

    if !succeeds(runner, top, &["add", "-A"]) {
        return SyncResult::new(Status::Fail, "git add failed");
    }
    if !succeeds(runner, top, &["commit", "-m", message]) {
        return SyncResult::new(Status::Fail, "git commit failed");
    }
    match push(runner, top, target) {
        Ok(()) => SyncResult::new(Status::Synced, "staged, committed, and pushed"),
        Err(detail) => SyncResult::new(Status::Fail, detail),
    }
}

/// Runs git and returns trimmed stdout on a clean exit with non-empty output, else `None`.
fn capture(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> Option<String> {
    match runner.run(repo, args) {
        Ok(output) if output.exit_code == 0 && !output.stdout.trim().is_empty() => {
            Some(output.stdout.trim().to_string())
        }
        _ => None,
    }
}

fn succeeds(runner: &impl GitRunner, repo: &Path, args: &[&str]) -> bool {
    matches!(runner.run(repo, args), Ok(output) if output.exit_code == 0)
}

fn push(runner: &impl GitRunner, repo: &Path, target: &SyncTarget) -> Result<(), String> {
    match runner.run(repo, &["push", &target.remote, &target.branch]) {
        Ok(output) if output.exit_code == 0 => Ok(()),
        Ok(output) => Err(format!("push failed (exit {})", output.exit_code)),
        Err(error) => Err(error.to_string()),
    }
}

fn repo_name(top: &str) -> String {
    Path::new(top)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::commands::squash_local::GitOutput;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        repo: PathBuf,
        args: Vec<String>,
    }

    struct FakeRunner {
        calls: RefCell<Vec<Call>>,
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
                stdout: stdout.to_string(),
                exit_code: 0,
            }
        }

        fn exit(stdout: &str, exit_code: i32) -> GitOutput {
            GitOutput {
                stdout: stdout.to_string(),
                exit_code,
            }
        }

        fn calls(&self) -> Vec<Call> {
            self.calls.borrow().clone()
        }

        fn arg_lists(&self) -> Vec<Vec<String>> {
            self.calls().into_iter().map(|call| call.args).collect()
        }
    }

    impl GitRunner for FakeRunner {
        fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
            self.calls.borrow_mut().push(Call {
                repo: repo.to_path_buf(),
                args: args.iter().map(|arg| arg.to_string()).collect(),
            });
            Ok(self.results.borrow_mut().remove(0))
        }
    }

    fn target() -> SyncTarget {
        SyncTarget {
            name: "repo-a".to_string(),
            top: "/home/me/work/repo-a".to_string(),
            branch: "main".to_string(),
            remote: "origin".to_string(),
            remote_url: "git@github.com:me/repo-a.git".to_string(),
        }
    }

    // --- gate ---------------------------------------------------------------

    #[test]
    fn gate_with_yes_proceeds_regardless_of_tty() {
        assert_eq!(gate(true, false), Gate::Proceed);
        assert_eq!(gate(true, true), Gate::Proceed);
    }

    #[test]
    fn gate_interactive_without_yes_asks_to_confirm() {
        assert_eq!(gate(false, true), Gate::Confirm);
    }

    #[test]
    fn gate_noninteractive_without_yes_refuses() {
        assert_eq!(gate(false, false), Gate::RefuseNonInteractive);
    }

    // --- confirmation -------------------------------------------------------

    #[test]
    fn confirmation_names_repo_branch_and_remote() {
        let text = confirmation(&target());
        assert!(text.contains("repo-a"), "names the repo");
        assert!(text.contains("/home/me/work/repo-a"), "shows the path");
        assert!(text.contains("main"), "names the branch");
        assert!(text.contains("origin"), "names the remote");
        assert!(
            text.contains("git@github.com:me/repo-a.git"),
            "shows the remote url"
        );
    }

    #[test]
    fn confirmation_omits_empty_remote_url_parens() {
        let mut t = target();
        t.remote_url = String::new();
        let text = confirmation(&t);
        assert!(text.contains("origin"));
        assert!(!text.contains("()"), "no empty parens when url is unknown");
    }

    // --- plan ---------------------------------------------------------------

    #[test]
    fn plan_refuses_when_not_a_git_repo() {
        let runner = FakeRunner::new(vec![FakeRunner::exit("", 128)]);
        let plan = plan(&runner, Path::new("."));
        assert_eq!(plan, Plan::Refused("not a git repo".to_string()));
    }

    #[test]
    fn plan_refuses_on_detached_head() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/work/repo-a\n"),
            FakeRunner::ok("HEAD\n"),
        ]);
        let plan = plan(&runner, Path::new("."));
        assert_eq!(
            plan,
            Plan::Refused("detached HEAD — checkout a branch first".to_string())
        );
    }

    #[test]
    fn plan_refuses_without_upstream() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/work/repo-a\n"),
            FakeRunner::ok("main\n"),
            FakeRunner::exit("", 1),
        ]);
        let plan = plan(&runner, Path::new("."));
        assert_eq!(
            plan,
            Plan::Refused("no upstream tracking branch (run: git push -u origin main)".to_string())
        );
    }

    #[test]
    fn plan_ready_collects_branch_remote_and_url() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/work/repo-a\n"),
            FakeRunner::ok("main\n"),
            FakeRunner::ok("origin\n"),
            FakeRunner::ok("git@github.com:me/repo-a.git\n"),
        ]);
        let plan = plan(&runner, Path::new("."));
        assert_eq!(plan, Plan::Ready(target()));
    }

    // --- apply --------------------------------------------------------------

    #[test]
    fn apply_commits_and_pushes_when_dirty() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(" M a.txt\n?? b.txt\n"), // status --porcelain: dirty
            FakeRunner::ok(""),                     // add -A
            FakeRunner::ok(""),                     // commit
            FakeRunner::ok(""),                     // push
        ]);
        let result = apply(&runner, &target(), "save work");
        assert_eq!(result.status, Status::Synced);
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["status", "--porcelain"],
                vec!["add", "-A"],
                vec!["commit", "-m", "save work"],
                vec!["push", "origin", "main"],
            ]
        );
    }

    #[test]
    fn apply_pushes_without_commit_when_clean_but_ahead() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""),    // status --porcelain: clean
            FakeRunner::ok("2\n"), // rev-list --count @{u}..HEAD
            FakeRunner::ok(""),    // push
        ]);
        let result = apply(&runner, &target(), "save work");
        assert_eq!(result.status, Status::Synced);
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["status", "--porcelain"],
                vec!["rev-list", "--count", "@{u}..HEAD"],
                vec!["push", "origin", "main"],
            ]
        );
    }

    #[test]
    fn apply_noops_when_clean_and_up_to_date() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""),    // status --porcelain: clean
            FakeRunner::ok("0\n"), // rev-list --count: nothing ahead
        ]);
        let result = apply(&runner, &target(), "save work");
        assert_eq!(result.status, Status::Noop);
        assert_eq!(
            runner.arg_lists(),
            vec![
                vec!["status", "--porcelain"],
                vec!["rev-list", "--count", "@{u}..HEAD"],
            ],
            "no push when clean and up to date"
        );
    }

    #[test]
    fn apply_fails_when_commit_fails_without_pushing() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(" M a.txt\n"), // dirty
            FakeRunner::ok(""),           // add -A
            FakeRunner::exit("", 1),      // commit fails
        ]);
        let result = apply(&runner, &target(), "save work");
        assert_eq!(result.status, Status::Fail);
        assert!(
            runner
                .arg_lists()
                .iter()
                .all(|args| args.first().map(String::as_str) != Some("push")),
            "push must not run after a failed commit"
        );
    }

    #[test]
    fn apply_fails_when_push_fails() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""),      // clean
            FakeRunner::ok("1\n"),   // one ahead
            FakeRunner::exit("", 1), // push fails
        ]);
        let result = apply(&runner, &target(), "save work");
        assert_eq!(result.status, Status::Fail);
    }
}
