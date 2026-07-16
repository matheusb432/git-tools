use std::{fmt::Write as _, path::Path};

use application::ports::GitRunner;

/// What a `sync` will push, gathered read-only for the confirmation prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTarget {
    pub name: String,
    pub top: String,
    pub branch: String,
    pub remote: String,
    pub remote_url: String,
    pub pending: Pending,
}

/// What a local-only commit will touch, gathered read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCommitTarget {
    pub name: String,
    pub top: String,
    pub branch: String,
    pub pending: Pending,
}

/// Read-only snapshot of what `push "<message>"` will sweep up, for the confirmation block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pending {
    /// Distinct working-tree paths with any change (staged, unstaged, or untracked).
    pub changed: usize,
    /// Paths with staged (already-prepared) changes in the index.
    pub staged: usize,
    /// Paths with unstaged or untracked (unprepared) changes `git add -A` will stage.
    pub unprepared: usize,
    /// Commits already ahead of the upstream (`@{u}..HEAD`).
    pub ahead: usize,
}

/// Outcome of the read-only planning pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Cannot sync; the string explains why (printed to stderr, exit 1).
    Refused(String),
    /// Ready to confirm and apply.
    Ready(SyncTarget),
}

/// Outcome of the read-only planning pass for local-only commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCommitPlan {
    /// Cannot commit locally; the string explains why (printed to stderr, exit 1).
    Refused(String),
    /// Ready to stage and commit.
    Ready(LocalCommitTarget),
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

/// Builds the review block printed before the current-repo stage/commit/push flow runs.
/// Spells out every side effect — what gets staged, what is wrapped into the
/// commit, and where it lands — so the user confirms an action, not just a repo.
pub fn confirmation(command: &str, target: &SyncTarget, message: &str) -> String {
    let remote = if target.remote_url.is_empty() {
        target.remote.clone()
    } else {
        format!("{} ({})", target.remote, target.remote_url)
    };
    let dest = format!("{}/{}", target.remote, target.branch);
    let Pending {
        changed,
        staged,
        unprepared,
        ahead,
    } = target.pending;

    let mut out = format!(
        "{command} — review before committing & pushing:\n  repo:    {} ({})\n  branch:  {}\n  remote:  {}\n  message: {}\n\n{command} will:",
        target.name, target.top, target.branch, remote, message
    );

    if changed > 0 {
        if unprepared > 0 {
            let _ = write!(
                out,
                "\n  • stage {unprepared} unprepared change(s) with `git add -A`"
            );
        }
        let staged_note = if staged > 0 {
            format!(" ({staged} already staged)")
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "\n  • commit {changed} change(s){staged_note} as a single commit"
        );
        let _ = write!(out, "\n  • push {} commit(s) to {dest}", ahead + 1);
    } else if ahead > 0 {
        let _ = write!(
            out,
            "\n  • nothing to commit; push {ahead} unpushed commit(s) to {dest}"
        );
    } else {
        out.push_str("\n  • nothing to commit or push — already up to date");
    }

    out
}

/// Builds the review block printed before a plain current-repo `push`.
/// This copy is intentionally push-only: it never implies staging or committing.
pub fn push_confirmation(target: &SyncTarget) -> String {
    let remote = if target.remote_url.is_empty() {
        target.remote.clone()
    } else {
        format!("{} ({})", target.remote, target.remote_url)
    };
    let dest = format!("{}/{}", target.remote, target.branch);
    let Pending { changed, ahead, .. } = target.pending;

    let mut out = format!(
        "push — review before pushing:\n  repo:    {} ({})\n  branch:  {}\n  remote:  {}\n\npush will:",
        target.name, target.top, target.branch, remote
    );

    if changed > 0 {
        let _ = write!(
            out,
            "\n  • refuse to push while {changed} uncommitted change(s) are present"
        );
        out.push_str("\n  • push only existing commits; it will not stage or create a commit");
    } else if ahead > 0 {
        let _ = write!(out, "\n  • push {ahead} unpushed commit(s) to {dest}");
    } else {
        out.push_str("\n  • nothing to push — already up to date");
    }

    out
}

/// Builds the review block printed before the current-repo local commit flow runs.
pub fn commit_confirmation(target: &LocalCommitTarget, message: &str) -> String {
    let Pending {
        changed,
        staged,
        unprepared,
        ahead: _,
    } = target.pending;

    let mut out = format!(
        "commit — review before committing:\n  repo:    {} ({})\n  branch:  {}\n  message: {}\n\ncommit will:",
        target.name, target.top, target.branch, message
    );

    if changed > 0 {
        if unprepared > 0 {
            let _ = write!(
                out,
                "\n  • stage {unprepared} unprepared change(s) with `git add -A`"
            );
        }
        let staged_note = if staged > 0 {
            format!(" ({staged} already staged)")
        } else {
            String::new()
        };
        let _ = write!(
            out,
            "\n  • commit {changed} change(s){staged_note} as a single commit"
        );
        out.push_str("\n  • leave the commit local; it will not push");
    } else {
        out.push_str("\n  • nothing to commit — working tree clean");
    }

    out
}

pub fn plan(runner: &impl GitRunner, repo: &Path) -> Plan {
    let Some(top) = capture(runner, repo, &["rev-parse", "--show-toplevel"]) else {
        return Plan::Refused("not a git repo".to_string());
    };
    let top_path = Path::new(&top);

    let branch = match capture(runner, top_path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch != "HEAD" => branch,
        _ => return Plan::Refused("detached HEAD — checkout a branch first".to_string()),
    };

    let Some(remote) = capture(
        runner,
        top_path,
        &["config", &format!("branch.{branch}.remote")],
    ) else {
        return Plan::Refused(format!(
            "no upstream tracking branch (run: git push -u origin {branch})"
        ));
    };

    // ? remote url is for the prompt only; an unset url must not block the sync.
    let remote_url = capture(runner, top_path, &["remote", "get-url", &remote]).unwrap_or_default();

    let porcelain = match runner.run(top_path, &["status", "--porcelain"]) {
        Ok(output) if output.exit_code == 0 => output.stdout,
        _ => return Plan::Refused("git status failed".to_string()),
    };
    let (changed, staged, unprepared) = classify(&porcelain);
    // ? ahead is for the prompt only; treat a probe failure as zero rather than refusing.
    let ahead = capture(runner, top_path, &["rev-list", "--count", "@{u}..HEAD"])
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or(0);

    Plan::Ready(SyncTarget {
        name: repo_name(&top),
        top,
        branch,
        remote,
        remote_url,
        pending: Pending {
            changed,
            staged,
            unprepared,
            ahead,
        },
    })
}

pub fn plan_local_commit(runner: &impl GitRunner, repo: &Path) -> LocalCommitPlan {
    let Some(top) = capture(runner, repo, &["rev-parse", "--show-toplevel"]) else {
        return LocalCommitPlan::Refused("not a git repo".to_string());
    };
    let top_path = Path::new(&top);

    let branch = match capture(runner, top_path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Some(branch) if branch != "HEAD" => branch,
        _ => {
            return LocalCommitPlan::Refused("detached HEAD — checkout a branch first".to_string());
        }
    };

    let porcelain = match runner.run(top_path, &["status", "--porcelain"]) {
        Ok(output) if output.exit_code == 0 => output.stdout,
        _ => return LocalCommitPlan::Refused("git status failed".to_string()),
    };
    let (changed, staged, unprepared) = classify(&porcelain);

    LocalCommitPlan::Ready(LocalCommitTarget {
        name: repo_name(&top),
        top,
        branch,
        pending: Pending {
            changed,
            staged,
            unprepared,
            ahead: 0,
        },
    })
}

/// Counts porcelain-v1 lines into (changed paths, staged paths, unprepared paths).
/// A path may be both staged and unprepared (index change plus later edits), so the
/// two columns can sum past `changed`; `changed` is the distinct-path total.
fn classify(porcelain: &str) -> (usize, usize, usize) {
    let mut changed = 0;
    let mut staged = 0;
    let mut unprepared = 0;
    for line in porcelain.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 2 {
            continue;
        }
        changed += 1;
        let (index, worktree) = (bytes[0], bytes[1]);
        if index == b'?' && worktree == b'?' {
            unprepared += 1;
            continue;
        }
        if index != b' ' {
            staged += 1;
        }
        if worktree != b' ' {
            unprepared += 1;
        }
    }
    (changed, staged, unprepared)
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

pub fn push_existing(runner: &impl GitRunner, target: &SyncTarget) -> SyncResult {
    let top = Path::new(&target.top);
    let dirty = match runner.run(top, &["status", "--porcelain"]) {
        Ok(output) if output.exit_code == 0 => !output.stdout.trim().is_empty(),
        _ => return SyncResult::new(Status::Fail, "git status failed"),
    };
    if dirty {
        return SyncResult::new(Status::Refused, "working tree has uncommitted changes");
    }
    let ahead = match capture(runner, top, &["rev-list", "--count", "@{u}..HEAD"]) {
        Some(count) => count.parse::<usize>().unwrap_or(0),
        None => return SyncResult::new(Status::Fail, "rev-list failed"),
    };
    if ahead == 0 {
        return SyncResult::new(Status::Noop, "already up to date");
    }
    match push(runner, top, target) {
        Ok(()) => SyncResult::new(Status::Synced, format!("pushed {ahead} commit(s)")),
        Err(detail) => SyncResult::new(Status::Fail, detail),
    }
}

pub fn commit_only(runner: &impl GitRunner, target: &SyncTarget, message: &str) -> SyncResult {
    commit_at(runner, Path::new(&target.top), message)
}

pub fn commit_local(
    runner: &impl GitRunner,
    target: &LocalCommitTarget,
    message: &str,
) -> SyncResult {
    commit_at(runner, Path::new(&target.top), message)
}

fn commit_at(runner: &impl GitRunner, top: &Path, message: &str) -> SyncResult {
    let dirty = match runner.run(top, &["status", "--porcelain"]) {
        Ok(output) if output.exit_code == 0 => !output.stdout.trim().is_empty(),
        _ => return SyncResult::new(Status::Fail, "git status failed"),
    };
    if !dirty {
        return SyncResult::new(Status::Noop, "nothing to commit");
    }
    if !succeeds(runner, top, &["add", "-A"]) {
        return SyncResult::new(Status::Fail, "git add failed");
    }
    if !succeeds(runner, top, &["commit", "-m", message]) {
        return SyncResult::new(Status::Fail, "git commit failed");
    }
    SyncResult::new(Status::Synced, "staged and committed")
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
        Ok(output) => Err(output.fail_detail("push failed")),
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
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use application::ports::GitOutput;

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Call {
        repo: PathBuf,
        args: Vec<String>,
    }

    #[derive(Clone)]
    struct FakeRunner {
        calls: Arc<Mutex<Vec<Call>>>,
        results: Arc<Mutex<Vec<GitOutput>>>,
    }

    impl FakeRunner {
        fn new(results: Vec<GitOutput>) -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                results: Arc::new(Mutex::new(results)),
            }
        }

        fn ok(stdout: &str) -> GitOutput {
            GitOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                exit_code: 0,
            }
        }

        fn exit(stdout: &str, exit_code: i32) -> GitOutput {
            GitOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                exit_code,
            }
        }

        fn calls(&self) -> Vec<Call> {
            self.calls.lock().unwrap().clone()
        }

        fn arg_lists(&self) -> Vec<Vec<String>> {
            self.calls().into_iter().map(|call| call.args).collect()
        }
    }

    impl GitRunner for FakeRunner {
        fn run(&self, repo: &Path, args: &[&str]) -> anyhow::Result<GitOutput> {
            self.calls.lock().unwrap().push(Call {
                repo: repo.to_path_buf(),
                args: args.iter().map(std::string::ToString::to_string).collect(),
            });
            Ok(self.results.lock().unwrap().remove(0))
        }
    }

    fn target() -> SyncTarget {
        SyncTarget {
            name: "repo-a".to_string(),
            top: "/home/me/work/repo-a".to_string(),
            branch: "main".to_string(),
            remote: "origin".to_string(),
            remote_url: "git@github.com:me/repo-a.git".to_string(),
            pending: Pending {
                changed: 2,
                staged: 0,
                unprepared: 2,
                ahead: 2,
            },
        }
    }

    fn local_commit_target() -> LocalCommitTarget {
        LocalCommitTarget {
            name: "repo-a".to_string(),
            top: "/home/me/work/repo-a".to_string(),
            branch: "main".to_string(),
            pending: Pending {
                changed: 3,
                staged: 1,
                unprepared: 2,
                ahead: 0,
            },
        }
    }

    // --- confirmation -------------------------------------------------------

    #[test]
    fn confirmation_names_repo_branch_remote_and_message() {
        let text = confirmation("push", &target(), "save work");
        assert!(text.contains("repo-a"), "names the repo");
        assert!(text.contains("/home/me/work/repo-a"), "shows the path");
        assert!(text.contains("main"), "names the branch");
        assert!(text.contains("origin"), "names the remote");
        assert!(
            text.contains("git@github.com:me/repo-a.git"),
            "shows the remote url"
        );
        assert!(text.contains("save work"), "shows the commit message");
    }

    #[test]
    fn confirmation_omits_empty_remote_url_parens() {
        let mut t = target();
        t.remote_url = String::new();
        let text = confirmation("push", &t, "save work");
        assert!(text.contains("origin"));
        // The `push will:` plan also uses parens for the staged note, so scope the
        // assertion to the remote line.
        let remote_line = text
            .lines()
            .find(|line| line.trim_start().starts_with("remote:"))
            .expect("has a remote line");
        assert!(
            !remote_line.contains("()"),
            "no empty parens when url is unknown"
        );
    }

    #[test]
    fn confirmation_spells_out_stage_commit_push_for_a_dirty_repo() {
        let mut t = target();
        t.pending = Pending {
            changed: 3,
            staged: 1,
            unprepared: 2,
            ahead: 1,
        };
        let text = confirmation("push", &t, "save work");
        assert!(text.contains("stage 2 unprepared change(s)"), "{text}");
        assert!(
            text.contains("commit 3 change(s) (1 already staged) as a single commit"),
            "{text}"
        );
        assert!(text.contains("push 2 commit(s) to origin/main"), "{text}");
    }

    #[test]
    fn confirmation_clean_but_ahead_says_push_only() {
        let mut t = target();
        t.pending = Pending {
            changed: 0,
            staged: 0,
            unprepared: 0,
            ahead: 3,
        };
        let text = confirmation("push", &t, "ignored");
        assert!(
            text.contains("nothing to commit; push 3 unpushed commit(s) to origin/main"),
            "{text}"
        );
    }

    #[test]
    fn confirmation_clean_and_up_to_date_says_nothing_to_do() {
        let mut t = target();
        t.pending = Pending::default();
        let text = confirmation("push", &t, "ignored");
        assert!(text.contains("already up to date"), "{text}");
    }

    #[test]
    fn push_only_confirmation_omits_message_and_commit_language() {
        let mut t = target();
        t.pending = Pending {
            changed: 0,
            staged: 0,
            unprepared: 0,
            ahead: 3,
        };

        let text = push_confirmation(&t);

        assert!(!text.contains("message:"), "{text}");
        assert!(!text.contains("committing & pushing"), "{text}");
        assert!(!text.contains("commit "), "{text}");
        assert!(text.contains("review before pushing"), "{text}");
        assert!(
            text.contains("push 3 unpushed commit(s) to origin/main"),
            "{text}"
        );
    }

    #[test]
    fn commit_confirmation_spells_out_local_only_commit() {
        let text = commit_confirmation(&local_commit_target(), "save work");

        assert!(text.contains("review before committing"), "{text}");
        assert!(text.contains("save work"), "{text}");
        assert!(text.contains("stage 2 unprepared change(s)"), "{text}");
        assert!(
            text.contains("commit 3 change(s) (1 already staged) as a single commit"),
            "{text}"
        );
        assert!(text.contains("leave the commit local"), "{text}");
        assert!(!text.contains("push "), "{text}");
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
    fn plan_ready_collects_branch_remote_url_and_pending() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("/home/me/work/repo-a\n"),
            FakeRunner::ok("main\n"),
            FakeRunner::ok("origin\n"),
            FakeRunner::ok("git@github.com:me/repo-a.git\n"),
            FakeRunner::ok(" M a.txt\n?? b.txt\n"), // status --porcelain: 2 unprepared
            FakeRunner::ok("2\n"),                  // rev-list --count @{u}..HEAD
        ]);
        let plan = plan(&runner, Path::new("."));
        assert_eq!(plan, Plan::Ready(target()));
    }

    #[test]
    fn classify_splits_staged_from_unprepared() {
        // staged-only, unstaged-only, staged+unstaged, untracked.
        let porcelain = "M  a.txt\n M b.txt\nMM c.txt\n?? d.txt\n";
        assert_eq!(classify(porcelain), (4, 2, 3));
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
    fn push_existing_pushes_ahead_clean_repo_without_commit() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(""),
            FakeRunner::ok("2\n"),
            FakeRunner::ok(""),
        ]);
        let target = target();

        let result = push_existing(&runner, &target);

        assert_eq!(result.status, Status::Synced);
        assert_eq!(result.detail, "pushed 2 commit(s)");
        let calls = runner.calls();
        assert!(
            calls
                .iter()
                .all(|call| call.args.first().map(String::as_str) != Some("commit"))
        );
    }

    #[test]
    fn commit_only_stages_and_commits_dirty_repo_without_push() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok(" M file.txt\n"),
            FakeRunner::ok(""),
            FakeRunner::ok(""),
        ]);
        let target = target();

        let result = commit_only(&runner, &target, "save work");

        assert_eq!(result.status, Status::Synced);
        assert_eq!(result.detail, "staged and committed");
        let calls = runner.calls();
        assert!(
            calls
                .iter()
                .all(|call| call.args.first().map(String::as_str) != Some("push"))
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
