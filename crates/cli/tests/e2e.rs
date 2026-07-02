//! End-to-end tests: build real temporary git repos, run the built binary, and assert on
//! exit codes, stdout/stderr, and the HTML artifacts written to the central store.
//!
//! These replace the PowerShell conformance harness. That harness froze the legacy Node/PS
//! originals as goldens to prove the Rust port matched them; the port is done and the diff
//! surface has since moved on (lean `diff` + `diff subrepos`, GTL-0002), so these pin
//! *current* behavior instead — one Rust test per scenario the harness fixtures covered.

use std::{
    path::{Path, PathBuf},
    process::Command as Git,
};

use assert_cmd::Command;
use predicates::{prelude::PredicateBooleanExt, str::contains};
use tempfile::TempDir;

mod common;

/// A throwaway git repo with an isolated central store.
struct Repo {
    _tmp: TempDir,
    _store: TempDir,
    root: PathBuf,
    repo: PathBuf,
    monorepo: PathBuf,
    store_dir: PathBuf,
}

struct NestedRepos {
    _tmp: TempDir,
    _store: TempDir,
    root: PathBuf,
    repos: Vec<PathBuf>,
    store_dir: PathBuf,
}

impl NestedRepos {
    fn new(names: &[&str]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let store_dir = store.path().to_path_buf();
        let root = tmp.path().to_path_buf();
        let repos = names.iter().map(|name| root.join(name)).collect::<Vec<_>>();

        for repo in &repos {
            std::fs::create_dir_all(repo).unwrap();
            let out = Git::new("git")
                .args(["init", "-b", "main"])
                .arg(repo)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git init failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            git_in(repo, &["config", "user.name", "E2E Bot"]);
            git_in(repo, &["config", "user.email", "e2e@example.invalid"]);
            git_in(repo, &["config", "commit.gpgsign", "false"]);
            git_in(repo, &["config", "core.autocrlf", "false"]);
            commit_in(repo, "README.md", "base\n", "chore: base");
            commit_in(repo, "README.md", "base\nlocal\n", "feat: local work");
        }

        Self {
            _tmp: tmp,
            _store: store,
            root,
            repos,
            store_dir,
        }
    }

    fn run(&self, args: &[&str]) -> Command {
        common::ensure_daemon_built();
        let mut cmd = Command::cargo_bin("git-tools").unwrap();
        cmd.args(args)
            .current_dir(&self.root)
            .env("GIT_TOOLS_NO_OPEN", "1")
            .env("GIT_TOOLS_DATA_DIR", &self.store_dir)
            // Diff commands spawn a per-store daemon; a short idle timeout reaps it.
            .env("GIT_TOOLS_DAEMON_IDLE_SECS", "2");
        cmd
    }
}

fn git_in(repo: &Path, args: &[&str]) -> String {
    let out = Git::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn commit_in(repo: &Path, file: &str, contents: &str, message: &str) -> String {
    let path = repo.join(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, contents).unwrap();
    git_in(repo, &["add", "-A"]);
    let out = Git::new("git")
        .arg("-C")
        .arg(repo)
        .args(["commit", "-m", message])
        .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "commit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    git_in(repo, &["rev-parse", "HEAD"])
}

fn add_upstream_for(repo: &Path, remote: &Path) {
    let out = Git::new("git")
        .args(["init", "--bare"])
        .arg(remote)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bare remote init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    git_in(repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git_in(repo, &["push", "-u", "origin", "HEAD"]);
}

impl Repo {
    /// Creates an initialized repo (branch `main`, deterministic identity) and a monorepo dir.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let store_dir = store.path().to_path_buf();
        let root = tmp.path().to_path_buf();
        let repo = root.join("repo");
        let monorepo = root.join("monorepo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&monorepo).unwrap();
        let this = Self {
            _tmp: tmp,
            _store: store,
            root,
            repo,
            monorepo,
            store_dir,
        };
        this.git(&["init", "-b", "main"]);
        this.git(&["config", "user.name", "E2E Bot"]);
        this.git(&["config", "user.email", "e2e@example.invalid"]);
        this.git(&["config", "commit.gpgsign", "false"]);
        this.git(&["config", "core.autocrlf", "false"]);
        this
    }

    /// Runs `git -C <repo> <args>`, asserting success, and returns trimmed stdout.
    fn git(&self, args: &[&str]) -> String {
        let out = Git::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// Writes `file` then commits it with a fixed date; returns the new full SHA.
    fn commit(&self, file: &str, contents: &str, message: &str) -> String {
        let path = self.repo.join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        self.git(&["add", "-A"]);
        let out = Git::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(["commit", "-m", message])
            .env("GIT_AUTHOR_DATE", "2026-01-01T12:00:00")
            .env("GIT_COMMITTER_DATE", "2026-01-01T12:00:00")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "commit failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        self.git(&["rev-parse", "HEAD"])
    }

    /// Adds a bare `origin` and pushes `main`, establishing an upstream (`@{u}`).
    fn add_upstream(&self) {
        let remote = self.root.join("origin.git");
        let out = Git::new("git")
            .args(["init", "--bare"])
            .arg(&remote)
            .output()
            .unwrap();
        assert!(out.status.success());
        self.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        self.git(&["push", "-u", "origin", "HEAD"]);
    }

    /// A `git-tools` invocation in the repo with browser-open and store location set.
    fn run(&self, args: &[&str]) -> Command {
        common::ensure_daemon_built();
        let mut cmd = Command::cargo_bin("git-tools").unwrap();
        cmd.args(args)
            .current_dir(&self.repo)
            .env("GIT_TOOLS_NO_OPEN", "1")
            .env("GIT_TOOLS_DATA_DIR", &self.store_dir)
            // Diff commands spawn a per-store daemon; a short idle timeout reaps it.
            .env("GIT_TOOLS_DAEMON_IDLE_SECS", "2");
        cmd
    }

    /// Number of commits ahead of upstream (`@{u}..HEAD`).
    fn unpushed_count(&self) -> usize {
        self.git(&["rev-list", "--count", "@{u}..HEAD"])
            .parse()
            .unwrap()
    }

    fn repo_arg(&self) -> &str {
        self.repo.to_str().unwrap()
    }
    fn monorepo_arg(&self) -> &str {
        self.monorepo.to_str().unwrap()
    }
}

/// Extract the artifact path from a "wrote <path>" stdout line.
fn artifact_from_stdout(stdout: &str) -> PathBuf {
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("wrote ") {
            return PathBuf::from(path.trim());
        }
    }
    panic!("no 'wrote <path>' line found in stdout:\n{stdout}");
}

/// Asserts the artifact exists and its HTML names the repo and branch.
fn assert_html(path: &Path, branch: &str) {
    assert!(path.exists(), "expected artifact at {}", path.display());
    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.contains("<html"), "artifact is not HTML");
    assert!(html.contains(branch), "artifact missing branch {branch}");
}

/// Runs a command, asserts success, and returns (stdout, stderr) as strings.
fn run_success(mut cmd: Command) -> (String, String) {
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        output.status.success(),
        "command failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    (stdout, stderr)
}

// --- squash-preview --------------------------------------------------------

#[test]
fn squash_preview_reports_unpushed_and_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nmore\n", "feat: one");
    repo.commit("b.txt", "new\n", "feat: two");

    let cmd = repo.run(&[
        "squash-preview",
        "--repo",
        repo.repo_arg(),
        "--monorepo",
        repo.monorepo_arg(),
    ]);
    let (stdout, _) = run_success(cmd);
    assert!(stdout.contains("squash-preview:"), "stdout: {stdout}");
    assert!(stdout.contains("2 unpushed commit(s)"), "stdout: {stdout}");
    assert!(stdout.contains("wrote"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    assert_html(&artifact, "main");
    // Verify nothing was written into the repo itself
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "artifacts must not land in the repo"
    );
}

#[test]
fn squash_preview_without_upstream_errors() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");

    repo.run(&[
        "squash-preview",
        "--repo",
        repo.repo_arg(),
        "--monorepo",
        repo.monorepo_arg(),
    ])
    .assert()
    .code(1)
    .stderr(contains("no upstream tracking branch"));
}

// --- diff (lean) -----------------------------------------------------------

#[test]
fn diff_unpushed_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");

    let cmd = repo.run(&["diff"]);
    let (stdout, _) = run_success(cmd);
    assert!(stdout.contains("diff-preview:"), "stdout: {stdout}");
    assert!(stdout.contains("unpushed commit(s)"), "stdout: {stdout}");
    assert!(stdout.contains("wrote"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    assert!(
        artifact.exists(),
        "artifact must exist at {}",
        artifact.display()
    );
    assert!(
        artifact.starts_with(&repo.store_dir),
        "artifact must be under store dir, got {}",
        artifact.display()
    );
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_unpushed_flag_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");

    let cmd = repo.run(&["diff", "--unpushed"]);
    let (stdout, _) = run_success(cmd);
    assert!(stdout.contains("diff-preview:"), "stdout: {stdout}");
    assert!(stdout.contains("unpushed commit(s)"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    assert!(artifact.exists());
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_with_no_commits_or_changes_warns_and_skips_render() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream(); // HEAD == @{u}: no unpushed commits, clean tree -> nothing to diff

    repo.run(&["diff"])
        .assert()
        .success()
        .stderr(contains("nothing to show"));
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "empty diff must not write an artifact"
    );
}

#[test]
fn diff_empty_range_warns_and_skips_render() {
    let repo = Repo::new();
    let head = repo.commit("a.txt", "base\n", "chore: base");

    repo.run(&["diff", &format!("{head}..{head}")])
        .assert()
        .success()
        .stderr(contains("nothing to show"));
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "empty range must not write an artifact"
    );
}

#[test]
fn diff_without_upstream_falls_back_to_main() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.git(&["checkout", "-b", "feature/local"]);
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");

    let cmd = repo.run(&["diff"]);
    let (stdout, stderr) = run_success(cmd);
    assert!(stderr.contains("falling back to main"), "stderr: {stderr}");
    assert!(
        stdout.contains("diff-preview: main..working"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("wrote"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    assert_html(&artifact, "feature/local");
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_base_commit_form_writes_artifact() {
    let repo = Repo::new();
    let base = repo.commit("a.txt", "base\n", "chore: base");
    repo.commit("a.txt", "base\nchange\n", "feat: change");

    let cmd = repo.run(&["diff", &base]);
    let (stdout, _) = run_success(cmd);
    assert!(stdout.contains("diff-preview:"), "stdout: {stdout}");
    assert!(stdout.contains("working"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    assert!(artifact.exists());
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_exact_range_form_writes_artifact() {
    let repo = Repo::new();
    let start = repo.commit("a.txt", "1\n", "chore: one");
    repo.commit("a.txt", "1\n2\n", "feat: two");
    let end = repo.commit("a.txt", "1\n2\n3\n", "feat: three");

    let cmd = repo.run(&["diff", &format!("{start}..{end}")]);
    let (stdout, _) = run_success(cmd);
    assert!(stdout.contains("diff-preview:"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    assert!(artifact.exists());
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_merge_flag_writes_three_dot_merge_preview() {
    let repo = Repo::new();
    repo.commit("shared.txt", "base\n", "chore: base");
    repo.git(&["checkout", "-b", "feature"]);
    repo.commit("feature.txt", "feature\n", "feat: branch file");
    repo.git(&["checkout", "main"]);
    repo.commit("main.txt", "main\n", "feat: main file");
    repo.git(&["checkout", "feature"]);

    let cmd = repo.run(&["diff", "--merge", "main"]);
    let (stdout, _) = run_success(cmd);
    assert!(
        stdout.contains("diff-preview: to merge into main"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("wrote"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    let html = std::fs::read_to_string(&artifact).unwrap();
    assert!(html.contains("git diff main...HEAD"));
    assert!(html.contains("feat: branch file"));
    assert!(html.contains("feature.txt"));
    assert!(!html.contains("main.txt"));
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_last_n_commits_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "1\n", "chore: one");
    repo.commit("a.txt", "1\n2\n", "feat: two");
    repo.commit("a.txt", "1\n2\n3\n", "feat: three");

    // `-l 2` diffs HEAD~2..HEAD: the last two commits.
    let cmd = repo.run(&["diff", "-l", "2"]);
    let (stdout, _) = run_success(cmd);
    assert!(
        stdout.contains("diff-preview: last 2 commit(s)"),
        "stdout: {stdout}"
    );

    let artifact = artifact_from_stdout(&stdout);
    assert!(artifact.exists());
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_bare_last_diffs_the_last_commit() {
    let repo = Repo::new();
    repo.commit("a.txt", "1\n", "chore: one");
    repo.commit("a.txt", "1\n2\n", "feat: two");

    // Bare `-l` defaults to the last commit only.
    let cmd = repo.run(&["diff", "-l"]);
    let (stdout, _) = run_success(cmd);
    assert!(
        stdout.contains("diff-preview: last 1 commit(s)"),
        "stdout: {stdout}"
    );

    let artifact = artifact_from_stdout(&stdout);
    assert!(artifact.exists());
    assert!(
        !repo.repo.join(".artifacts").exists(),
        "no .artifacts in repo"
    );
}

#[test]
fn diff_artifact_embeds_full_file_context_for_modified_files() {
    let repo = Repo::new();
    let middle = "middle that stays hidden in compact diff";
    let base_contents = format!(
        "one\n{}\nlast\n",
        (0..20)
            .map(|i| if i == 10 {
                middle.to_string()
            } else {
                format!("filler-{i}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
    let changed_contents = base_contents
        .replacen("one", "ONE", 1)
        .replacen("last", "LAST", 1);
    let base = repo.commit("src/lib.rs", &base_contents, "chore: base");
    repo.commit("src/lib.rs", &changed_contents, "feat: touch distant lines");

    let cmd = repo.run(&["diff", &base]);
    let (stdout, _) = run_success(cmd);

    let artifact = artifact_from_stdout(&stdout);
    let html = std::fs::read_to_string(&artifact).unwrap();
    assert!(html.contains(r#"class="view-toggle""#));
    assert!(html.contains(r#"class="diff diff-unified diff-full""#));
    assert!(html.contains("middle that stays hidden in compact diff"));
}

#[test]
fn diff_last_beyond_history_errors() {
    let repo = Repo::new();
    repo.commit("a.txt", "1\n", "chore: one");

    // Asking for more commits than exist fails honestly: HEAD~5 is not a commit.
    repo.run(&["diff", "-l", "5"])
        .assert()
        .code(1)
        .stderr(contains("not a commit: HEAD~5"));
}

#[test]
fn diff_unknown_commit_errors() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");

    repo.run(&["diff", "deadbeef"])
        .assert()
        .code(1)
        .stderr(contains("not a commit: deadbeef"));
}

#[test]
fn diff_outside_a_git_repo_errors() {
    common::ensure_daemon_built();
    let tmp = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    Command::cargo_bin("git-tools")
        .unwrap()
        .arg("diff")
        .current_dir(tmp.path())
        .env("GIT_TOOLS_NO_OPEN", "1")
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .env("GIT_TOOLS_DAEMON_IDLE_SECS", "2")
        .assert()
        .code(1)
        .stderr(contains("not a git repo"));
}

// --- wk --------------------------------------------------------------------

#[test]
fn wk_base_prints_the_primary_worktree_path() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    let linked = repo.root.join("feature-wt");
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "feature/wk",
        linked.to_str().unwrap(),
    ]);

    Command::cargo_bin("git-tools")
        .unwrap()
        .args(["wk", "base"])
        .current_dir(&linked)
        .assert()
        .success()
        .stdout(contains(repo.repo.to_str().unwrap()))
        .stdout(contains("feature-wt").not());
}

#[test]
fn wk_ls_lists_worktrees_as_a_readable_table() {
    let repo = Repo::new();
    let head = repo.commit("a.txt", "base\n", "chore: base");
    let linked = repo.root.join("feature-wt");
    repo.git(&[
        "worktree",
        "add",
        "-b",
        "feature/wk",
        linked.to_str().unwrap(),
    ]);

    repo.run(&["wk", "ls"])
        .assert()
        .success()
        .stdout(contains("PATH"))
        .stdout(contains("BRANCH"))
        .stdout(contains("HEAD"))
        .stdout(contains("STATE"))
        .stdout(contains(repo.repo.to_str().unwrap()))
        .stdout(contains(linked.to_str().unwrap()))
        .stdout(contains("main"))
        .stdout(contains("feature/wk"))
        .stdout(contains(&head[..7]))
        .stdout(contains("primary"))
        .stdout(contains("linked"));
}

// --- up --------------------------------------------------------------------

#[test]
fn up_with_yes_commits_and_pushes_dirty_repo() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    std::fs::write(repo.repo.join("a.txt"), "base\nlocal\n").unwrap();

    repo.run(&["up", "save work", "--yes"])
        .assert()
        .success()
        .stdout(contains("up — review before committing & pushing"))
        .stdout(contains("message: save work"))
        .stdout(contains("main"))
        .stdout(contains("origin"))
        .stdout(contains("commit 1 change(s)"))
        .stdout(contains("staged, committed, and pushed"));

    assert_eq!(
        repo.git(&["status", "--porcelain"]),
        "",
        "tree is clean after sync"
    );
    assert_eq!(repo.unpushed_count(), 0, "the commit was pushed to origin");
}

#[test]
fn up_with_yes_pushes_clean_but_unpushed_commits() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: already committed");

    repo.run(&["up", "ignored message", "--yes"])
        .assert()
        .success()
        .stdout(contains("nothing to commit; pushed"));

    assert_eq!(repo.unpushed_count(), 0);
}

#[test]
fn up_noops_when_clean_and_up_to_date() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();

    repo.run(&["up", "nothing to do", "--yes"])
        .assert()
        .success()
        .stdout(contains("already up to date"));
}

#[test]
fn up_without_yes_refuses_in_noninteractive_shell() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    std::fs::write(repo.repo.join("a.txt"), "base\nlocal\n").unwrap();

    // assert_cmd runs without a TTY: the gate must refuse rather than auto-push.
    repo.run(&["up", "save work"])
        .assert()
        .code(2)
        .stderr(contains("pass --yes"));

    assert_ne!(
        repo.git(&["status", "--porcelain"]),
        "",
        "nothing was committed; the tree is still dirty"
    );
    assert_eq!(repo.unpushed_count(), 0, "nothing was committed or pushed");
}

#[test]
fn up_outside_a_git_repo_errors() {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("git-tools")
        .unwrap()
        .args(["up", "save work", "--yes"])
        .current_dir(tmp.path())
        .assert()
        .code(1)
        .stderr(contains("not a git repo"));
}

// --- tag -------------------------------------------------------------------

#[test]
fn tag_lists_all_tags_by_default() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.git(&["tag", "v1.0.0"]);
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    repo.git(&["tag", "v1.1.0"]);

    repo.run(&["tag"])
        .assert()
        .success()
        .stdout(contains("v1.0.0"))
        .stdout(contains("v1.1.0"));
}

#[test]
fn tag_ls_marks_fetched_remote_tags() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.git(&["tag", "v1.0.0"]);
    repo.git(&["push", "origin", "refs/tags/v1.0.0:refs/tags/v1.0.0"]);
    repo.git(&["fetch", "origin", "+refs/tags/*:refs/remotes/origin/tags/*"]);
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    repo.git(&["tag", "v1.1.0"]);

    repo.run(&["tag", "ls"])
        .assert()
        .success()
        .stdout(contains("v1.0.0 [remote]"))
        .stdout(contains("v1.1.0 [local]"));
}

#[test]
fn tag_ls_shows_annotated_tag_message_first_line() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.git(&[
        "tag",
        "-a",
        "v1.0.0",
        "-m",
        "ship release\n\nbody line ignored",
    ]);
    repo.git(&["tag", "lightweight"]);

    repo.run(&["tag", "ls"])
        .assert()
        .success()
        .stdout(contains("v1.0.0 [local]  ship release"))
        .stdout(contains("body line ignored").not())
        .stdout(contains("lightweight [local]"));
}

#[test]
fn tag_commits_lists_tags_with_target_commits() {
    let repo = Repo::new();
    let first = repo.commit("a.txt", "base\n", "chore: base");
    repo.git(&["tag", "v1.0.0"]);
    let second = repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    repo.git(&["tag", "v1.1.0"]);

    repo.run(&["tag", "--commits"])
        .assert()
        .success()
        .stdout(contains(&first[..7]))
        .stdout(contains("v1.0.0"))
        .stdout(contains(&second[..7]))
        .stdout(contains("v1.1.0"));
}

#[test]
fn tag_commits_peels_annotated_tags_to_commits() {
    let repo = Repo::new();
    let commit = repo.commit("a.txt", "base\n", "chore: base");
    repo.git(&["tag", "-a", "v1.0.0", "-m", "release"]);
    let tag_object = repo.git(&["rev-parse", "v1.0.0"]);

    repo.run(&["tag", "--commits"])
        .assert()
        .success()
        .stdout(contains(&commit[..7]))
        .stdout(predicates::str::contains(&tag_object[..7]).not());
}

#[test]
fn tag_add_creates_annotated_tag() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");

    repo.run(&["tag", "add", "v1.2.0", "release notes"])
        .assert()
        .success()
        .stdout(contains("created tag v1.2.0"));

    let tag_type = repo.git(&["cat-file", "-t", "v1.2.0"]);
    let tag_text = repo.git(&["cat-file", "-p", "v1.2.0"]);
    assert_eq!(tag_type, "tag");
    assert!(tag_text.contains("release notes"));
}

#[test]
fn tag_up_pushes_tags_to_origin() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.git(&["tag", "v1.0.0"]);

    repo.run(&["tag", "up"])
        .assert()
        .success()
        .stdout(contains("pushed 1 tag: v1.0.0"));

    let tags = repo.git(&["ls-remote", "--tags", "origin"]);
    assert!(tags.contains("refs/tags/v1.0.0"));
}

#[test]
fn tag_up_with_tag_and_message_creates_and_pushes_tag() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.git(&["tag", "v1.1.0"]);

    repo.run(&["tag", "up", "v1.2.0", "release notes"])
        .assert()
        .success()
        .stdout(contains("created tag v1.2.0"))
        .stdout(contains("pushed 1 tag: v1.2.0"))
        .stdout(contains("v1.1.0").not());

    let tag_type = repo.git(&["cat-file", "-t", "v1.2.0"]);
    let tag_text = repo.git(&["cat-file", "-p", "v1.2.0"]);
    let tags = repo.git(&["ls-remote", "--tags", "origin"]);
    assert_eq!(tag_type, "tag");
    assert!(tag_text.contains("release notes"));
    assert!(tags.contains("refs/tags/v1.2.0"));
    assert!(!tags.contains("refs/tags/v1.1.0"));

    repo.run(&["tag", "up"])
        .assert()
        .success()
        .stdout(contains("pushed 1 tag: v1.1.0"));
}

#[test]
fn tag_up_skips_when_fetched_remote_tags_are_current() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.git(&["tag", "v1.0.0"]);
    repo.git(&["push", "origin", "refs/tags/v1.0.0:refs/tags/v1.0.0"]);
    repo.git(&["fetch", "origin", "+refs/tags/*:refs/remotes/origin/tags/*"]);

    repo.run(&["tag", "up"])
        .assert()
        .success()
        .stdout(contains("tags already up to date"))
        .stdout(contains("pushed tags").not());
}

#[test]
fn tag_up_pushes_only_tags_missing_from_fetched_remote_tags() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.git(&["tag", "v1.0.0"]);
    repo.git(&["push", "origin", "refs/tags/v1.0.0:refs/tags/v1.0.0"]);
    repo.git(&["fetch", "origin", "+refs/tags/*:refs/remotes/origin/tags/*"]);
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    repo.git(&["tag", "v1.1.0"]);

    repo.run(&["tag", "up"])
        .assert()
        .success()
        .stdout(contains("pushed 1 tag: v1.1.0"))
        .stdout(contains("v1.0.0").not());

    let tags = repo.git(&["ls-remote", "--tags", "origin"]);
    assert!(tags.contains("refs/tags/v1.0.0"));
    assert!(tags.contains("refs/tags/v1.1.0"));

    repo.run(&["tag", "up"])
        .assert()
        .success()
        .stdout(contains("tags already up to date"));
}

// --- managed status --------------------------------------------------------

#[test]
fn status_lists_managed_repos_with_compact_ahead_dirty_and_untracked_symbols() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    std::fs::write(repo.repo.join("a.txt"), "base\nlocal\ndirty\n").unwrap();
    std::fs::write(repo.repo.join("scratch.txt"), "untracked\n").unwrap();
    let manifest = repo.root.join("repos.toml");
    std::fs::write(
        &manifest,
        "[[repo]]\npath = \"repo\"\nremote = \"\"\n\n[[repo]]\npath = \"missing\"\nremote = \"\"\n",
    )
    .unwrap();

    Command::cargo_bin("git-tools")
        .unwrap()
        .args([
            "status",
            "--all",
            "--repos-file",
            manifest.to_str().unwrap(),
            "--home-dir",
            repo.root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("repo main [⇡1 !?]"))
        .stdout(contains("missing (absent) [not present]"));
}

#[test]
fn status_color_always_bolds_brackets_and_marks_dirty_symbols_red() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");
    std::fs::write(repo.repo.join("a.txt"), "base\nlocal\ndirty\n").unwrap();
    std::fs::write(repo.repo.join("scratch.txt"), "untracked\n").unwrap();
    let manifest = repo.root.join("repos.toml");
    std::fs::write(&manifest, "[[repo]]\npath = \"repo\"\nremote = \"\"\n").unwrap();

    Command::cargo_bin("git-tools")
        .unwrap()
        .args([
            "status",
            "--all",
            "--color",
            "always",
            "--repos-file",
            manifest.to_str().unwrap(),
            "--home-dir",
            repo.root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains(
            "repo main \u{1b}[1m\u{1b}[38;2;242;133;0m[\u{1b}[39m\u{1b}[38;2;242;133;0m⇡\u{1b}[39m1 \u{1b}[38;2;255;77;77m!?\u{1b}[39m\u{1b}[38;2;242;133;0m]\u{1b}[39m\u{1b}[0m",
        ));
}

#[test]
fn status_color_always_bolds_brackets_and_marks_clean_checkmark_green() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    let manifest = repo.root.join("repos.toml");
    std::fs::write(&manifest, "[[repo]]\npath = \"repo\"\nremote = \"\"\n").unwrap();

    Command::cargo_bin("git-tools")
        .unwrap()
        .args([
            "ls",
            "--color",
            "always",
            "--repos-file",
            manifest.to_str().unwrap(),
            "--home-dir",
            repo.root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains(
            "repo main \u{1b}[1m\u{1b}[38;2;242;133;0m[\u{1b}[39m\u{1b}[38;2;46;204;113m✓\u{1b}[39m\u{1b}[38;2;242;133;0m]\u{1b}[39m\u{1b}[0m",
        ));
}

#[test]
fn status_default_reports_only_the_current_repo() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");

    // No --repos-file: the default scope is the current repo, never the manifest.
    repo.run(&["status"])
        .assert()
        .success()
        .stdout(contains("repo main [⇡1]"));
}

#[test]
fn status_recursive_lists_nested_subrepos() {
    let repos = NestedRepos::new(&["api", "web"]);

    repos
        .run(&["status", "-r"])
        .assert()
        .success()
        .stdout(contains("api main"))
        .stdout(contains("web main"));
}

#[test]
fn status_rejects_all_combined_with_recursive() {
    let repo = Repo::new();

    repo.run(&["status", "--all", "-r"])
        .assert()
        .failure()
        .stderr(contains("cannot be used with"));
}

// --- diff subrepos ---------------------------------------------------------

#[test]
fn diff_subrepos_nested_last_writes_one_tabbed_artifact() {
    let repos = NestedRepos::new(&["api", "web"]);

    let cmd = repos.run(&["diff", "subrepos", "-l"]);
    let (stdout, _) = run_success(cmd);
    assert!(
        stdout.contains("diff subrepos: 2 repo(s)"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("wrote"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    let html = std::fs::read_to_string(&artifact).unwrap();
    assert_eq!(
        html.matches(r#"<button class="tab"#).count(),
        repos.repos.len()
    );
    assert!(html.contains("api"));
    assert!(html.contains("web"));
    assert!(html.contains("diff-preview"));
}

#[test]
fn diff_subrepos_skips_nested_worktrees_by_default_and_includes_with_flag() {
    let repos = NestedRepos::new(&["api", "web"]);
    git_in(
        &repos.repos[0],
        &["worktree", "add", "-b", "feature", ".worktrees/feature"],
    );

    repos
        .run(&["diff", "subrepos", "-l"])
        .assert()
        .success()
        .stdout(contains("diff subrepos: 2 repo(s)"));

    repos
        .run(&["diff", "subrepos", "-l", "--worktrees"])
        .assert()
        .success()
        .stdout(contains("diff subrepos: 3 repo(s)"));
}

#[test]
fn diff_subrepos_without_upstreams_falls_back_to_main() {
    let repos = NestedRepos::new(&["api", "web"]);
    for repo in &repos.repos {
        git_in(repo, &["reset", "--hard", "HEAD~1"]);
        git_in(repo, &["checkout", "-b", "feature/local"]);
        commit_in(
            repo,
            "README.md",
            "base\nlocal branch\n",
            "feat: local branch",
        );
    }

    let cmd = repos.run(&["diff", "subrepos"]);
    let (stdout, _) = run_success(cmd);
    assert!(
        stdout.contains("diff subrepos: 2 repo(s)"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("wrote"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    let html = std::fs::read_to_string(&artifact).unwrap();
    assert_eq!(html.matches(r#"<button class="tab"#).count(), 2);
    assert!(html.contains("feature/local"));
    assert!(html.contains("main"));
    assert!(html.contains("feat: local branch"));
}

#[test]
fn diff_subrepos_skips_repos_with_nothing_to_show() {
    let repos = NestedRepos::new(&["api", "web"]);
    // api: up to date with its upstream — nothing unpushed, so it has nothing to show.
    add_upstream_for(&repos.repos[0], &repos.root.join("api.git"));
    // web: one commit ahead of its upstream — real unpushed work to preview.
    add_upstream_for(&repos.repos[1], &repos.root.join("web.git"));
    commit_in(
        &repos.repos[1],
        "README.md",
        "base\nlocal\nweb unpushed\n",
        "feat: queued work",
    );

    let (stdout, stderr) = run_success(repos.run(&["diff", "subrepos"]));

    // Only the repo with work is rendered; the empty one is reported, not tabbed.
    assert!(
        stdout.contains("diff subrepos: 1 repo(s)"),
        "stdout: {stdout}"
    );
    assert!(
        stderr.contains("skipped 1 repo(s) with nothing to show"),
        "skip summary missing from stderr: {stderr}"
    );

    let artifact = artifact_from_stdout(&stdout);
    let html = std::fs::read_to_string(&artifact).unwrap();
    assert_eq!(
        html.matches(r#"<button class="tab"#).count(),
        1,
        "only the non-empty repo should have a tab"
    );
    assert!(html.contains("feat: queued work"), "renders web's work");
}

#[test]
fn diff_subrepos_all_empty_writes_no_preview() {
    let repos = NestedRepos::new(&["api", "web"]);
    // Both repos are up to date with their upstreams — nothing to show anywhere.
    add_upstream_for(&repos.repos[0], &repos.root.join("api.git"));
    add_upstream_for(&repos.repos[1], &repos.root.join("web.git"));

    let (stdout, stderr) = run_success(repos.run(&["diff", "subrepos"]));

    // No artifact is written, and the empty result is surfaced in the terminal.
    assert!(
        !stdout.contains("wrote "),
        "no preview should be written: {stdout}"
    );
    assert!(
        stderr.contains("nothing to show across 2 repo(s)"),
        "stderr: {stderr}"
    );
}

#[test]
fn diff_all_writes_one_tabbed_artifact_for_managed_unpushed_repos() {
    let repos = NestedRepos::new(&["api", "web"]);
    for repo in &repos.repos {
        let name = repo.file_name().unwrap().to_string_lossy();
        add_upstream_for(repo, &repos.root.join(format!("{name}.git")));
        commit_in(
            repo,
            "README.md",
            &format!("base\nlocal\n{name} unpushed\n"),
            "feat: queued work",
        );
    }
    let manifest = repos.root.join("repos.toml");
    std::fs::write(
        &manifest,
        "[[repo]]\npath = \"api\"\nremote = \"\"\n\n[[repo]]\npath = \"web\"\nremote = \"\"\n",
    )
    .unwrap();

    let cmd = repos.run(&[
        "diff",
        "--all",
        "--repos-file",
        manifest.to_str().unwrap(),
        "--home-dir",
        repos.root.to_str().unwrap(),
    ]);
    let (stdout, _) = run_success(cmd);
    assert!(stdout.contains("diff-all: 2 repo(s)"), "stdout: {stdout}");
    assert!(stdout.contains("wrote"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    let html = std::fs::read_to_string(&artifact).unwrap();
    assert_eq!(html.matches(r#"<button class="tab"#).count(), 2);
    assert!(html.contains("api"));
    assert!(html.contains("web"));
    assert!(html.contains("feat: queued work"));
}

// --- merge-diff ------------------------------------------------------------

#[test]
fn merge_diff_against_base_branch_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "root\n", "chore: root");
    // Advance a feature branch beyond main, then preview merging it back.
    repo.git(&["checkout", "-b", "feature"]);
    repo.commit("feature.txt", "f1\n", "feat: branch file");
    repo.commit("feature.txt", "f1\nf2\n", "feat: extend branch file");

    let cmd = repo.run(&[
        "merge-diff",
        "--repo",
        repo.repo_arg(),
        "--monorepo",
        repo.monorepo_arg(),
        "--base",
        "main",
    ]);
    let (stdout, _) = run_success(cmd);
    assert!(stdout.contains("merge-diff:"), "stdout: {stdout}");
    assert!(stdout.contains("to merge into main"), "stdout: {stdout}");

    let artifact = artifact_from_stdout(&stdout);
    assert_html(&artifact, "feature");
    assert!(
        !repo.monorepo.join(".artifacts").exists(),
        "no .artifacts in monorepo"
    );
}

// --- squash-local ----------------------------------------------------------

#[test]
fn squash_local_dry_previews_without_rewriting() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\n1\n", "feat: one");
    repo.commit("a.txt", "base\n1\n2\n", "feat: two");

    repo.run(&[
        "squash-local",
        "collapse work",
        "--repo",
        repo.repo_arg(),
        "--dry",
    ])
    .assert()
    .success()
    .stdout(contains(
        "[dry] would collapse 2 unpushed commits into one:",
    ))
    .stdout(contains("[dry] new message would be: collapse work"));
    // Dry run leaves history untouched.
    assert_eq!(repo.unpushed_count(), 2);
}

#[test]
fn squash_local_collapses_unpushed_commits() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\n1\n", "feat: one");
    repo.commit("a.txt", "base\n1\n2\n", "feat: two");

    repo.run(&["squash-local", "collapse work", "--repo", repo.repo_arg()])
        .assert()
        .success()
        .stdout(contains("squashed 2 commits into one."))
        .stdout(contains("recover: git reset --soft"));
    // Now exactly one commit ahead, carrying the new message.
    assert_eq!(repo.unpushed_count(), 1);
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "collapse work");
}

#[test]
fn squash_local_noop_when_nothing_unpushed() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();

    repo.run(&["squash-local", "noop", "--repo", repo.repo_arg()])
        .assert()
        .success()
        .stdout(contains("nothing unpushed"));
}

#[test]
fn squash_local_refused_without_upstream() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.commit("a.txt", "base\nmore\n", "feat: more");

    repo.run(&["squash-local", "collapse", "--repo", repo.repo_arg()])
        .assert()
        .code(1)
        .stderr(contains("refused: no upstream tracking branch"));
}
