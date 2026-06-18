//! End-to-end tests: build real temporary git repos, run the built binary, and assert on
//! exit codes, stdout/stderr, and the HTML artifacts written under `<monorepo>/.artifacts`.
//!
//! These replace the PowerShell conformance harness. That harness froze the legacy Node/PS
//! originals as goldens to prove the Rust port matched them; the port is done and the diff
//! surface has since moved on (lean `diff` + `diff subrepos`, GTL-0002), so these pin
//! *current* behavior instead — one Rust test per scenario the harness fixtures covered.

use assert_cmd::Command;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use std::process::Command as Git;
use tempfile::TempDir;

/// A throwaway git repo plus a sibling monorepo dir that receives `.artifacts/`.
struct Repo {
    _tmp: TempDir,
    root: PathBuf,
    repo: PathBuf,
    monorepo: PathBuf,
}

struct NestedRepos {
    _tmp: TempDir,
    root: PathBuf,
    repos: Vec<PathBuf>,
}

impl NestedRepos {
    fn new(names: &[&str]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
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
            root,
            repos,
        }
    }

    fn run(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("git-tools").unwrap();
        cmd.args(args)
            .current_dir(&self.root)
            .env("GIT_TOOLS_NO_OPEN", "1");
        cmd
    }

    fn artifact(&self) -> PathBuf {
        self.root.join(".artifacts/diff-preview-subrepos.html")
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
        let root = tmp.path().to_path_buf();
        let repo = root.join("repo");
        let monorepo = root.join("monorepo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&monorepo).unwrap();
        let this = Self {
            _tmp: tmp,
            root,
            repo,
            monorepo,
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

    /// A `git-tools` invocation in the repo with the browser-open side effect suppressed.
    fn run(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("git-tools").unwrap();
        cmd.args(args)
            .current_dir(&self.repo)
            .env("GIT_TOOLS_NO_OPEN", "1");
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
    fn artifact(&self, name: &str) -> PathBuf {
        self.monorepo.join(".artifacts").join(name)
    }
}

/// Asserts the artifact exists and its HTML names the repo and branch.
fn assert_html(path: &Path, branch: &str) {
    assert!(path.exists(), "expected artifact at {}", path.display());
    let html = std::fs::read_to_string(path).unwrap();
    assert!(html.contains("<html"), "artifact is not HTML");
    assert!(html.contains(branch), "artifact missing branch {branch}");
}

// --- squash-preview --------------------------------------------------------

#[test]
fn squash_preview_reports_unpushed_and_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nmore\n", "feat: one");
    repo.commit("b.txt", "new\n", "feat: two");

    repo.run(&[
        "squash-preview",
        "--repo",
        repo.repo_arg(),
        "--monorepo",
        repo.monorepo_arg(),
    ])
    .assert()
    .success()
    .stdout(contains("squash-preview:"))
    .stdout(contains("2 unpushed commit(s)"))
    .stdout(contains("wrote"));
    assert_html(&repo.artifact("squash-preview-repo.html"), "main");
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

    // Lean diff writes under the current repo's own .artifacts (monorepo == repo).
    repo.run(&["diff"])
        .assert()
        .success()
        .stdout(contains("diff-preview:"))
        .stdout(contains("unpushed commit(s)"))
        .stdout(contains("wrote"));
    assert!(repo.repo.join(".artifacts/diff-preview-repo.html").exists());
}

#[test]
fn diff_unpushed_flag_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: local work");

    repo.run(&["diff", "--unpushed"])
        .assert()
        .success()
        .stdout(contains("diff-preview:"))
        .stdout(contains("unpushed commit(s)"))
        .stdout(contains("wrote"));
    assert!(repo.repo.join(".artifacts/diff-preview-repo.html").exists());
}

#[test]
fn diff_base_commit_form_writes_artifact() {
    let repo = Repo::new();
    let base = repo.commit("a.txt", "base\n", "chore: base");
    repo.commit("a.txt", "base\nchange\n", "feat: change");

    repo.run(&["diff", &base])
        .assert()
        .success()
        .stdout(contains("diff-preview:"))
        .stdout(contains("working"));
    assert!(repo.repo.join(".artifacts/diff-preview-repo.html").exists());
}

#[test]
fn diff_exact_range_form_writes_artifact() {
    let repo = Repo::new();
    let start = repo.commit("a.txt", "1\n", "chore: one");
    repo.commit("a.txt", "1\n2\n", "feat: two");
    let end = repo.commit("a.txt", "1\n2\n3\n", "feat: three");

    repo.run(&["diff", &format!("{start}..{end}")])
        .assert()
        .success()
        .stdout(contains("diff-preview:"));
    assert!(repo.repo.join(".artifacts/diff-preview-repo.html").exists());
}

#[test]
fn diff_last_n_commits_writes_artifact() {
    let repo = Repo::new();
    repo.commit("a.txt", "1\n", "chore: one");
    repo.commit("a.txt", "1\n2\n", "feat: two");
    repo.commit("a.txt", "1\n2\n3\n", "feat: three");

    // `-l 2` diffs HEAD~2..HEAD: the last two commits.
    repo.run(&["diff", "-l", "2"])
        .assert()
        .success()
        .stdout(contains("diff-preview: last 2 commit(s)"));
    assert!(repo.repo.join(".artifacts/diff-preview-repo.html").exists());
}

#[test]
fn diff_bare_last_diffs_the_last_commit() {
    let repo = Repo::new();
    repo.commit("a.txt", "1\n", "chore: one");
    repo.commit("a.txt", "1\n2\n", "feat: two");

    // Bare `-l` defaults to the last commit only.
    repo.run(&["diff", "-l"])
        .assert()
        .success()
        .stdout(contains("diff-preview: last 1 commit(s)"));
    assert!(repo.repo.join(".artifacts/diff-preview-repo.html").exists());
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
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("git-tools")
        .unwrap()
        .arg("diff")
        .current_dir(tmp.path())
        .env("GIT_TOOLS_NO_OPEN", "1")
        .assert()
        .code(1)
        .stderr(contains("not a git repo"));
}

// --- sync ------------------------------------------------------------------

#[test]
fn sync_with_yes_commits_and_pushes_dirty_repo() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    std::fs::write(repo.repo.join("a.txt"), "base\nlocal\n").unwrap();

    repo.run(&["sync", "save work", "--yes"])
        .assert()
        .success()
        .stdout(contains("sync — review before pushing"))
        .stdout(contains("main"))
        .stdout(contains("origin"))
        .stdout(contains("staged, committed, and pushed"));

    assert_eq!(
        repo.git(&["status", "--porcelain"]),
        "",
        "tree is clean after sync"
    );
    assert_eq!(repo.unpushed_count(), 0, "the commit was pushed to origin");
}

#[test]
fn sync_with_yes_pushes_clean_but_unpushed_commits() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    repo.commit("a.txt", "base\nlocal\n", "feat: already committed");

    repo.run(&["sync", "ignored message", "--yes"])
        .assert()
        .success()
        .stdout(contains("nothing to commit; pushed"));

    assert_eq!(repo.unpushed_count(), 0);
}

#[test]
fn sync_noops_when_clean_and_up_to_date() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();

    repo.run(&["sync", "nothing to do", "--yes"])
        .assert()
        .success()
        .stdout(contains("already up to date"));
}

#[test]
fn sync_without_yes_refuses_in_noninteractive_shell() {
    let repo = Repo::new();
    repo.commit("a.txt", "base\n", "chore: base");
    repo.add_upstream();
    std::fs::write(repo.repo.join("a.txt"), "base\nlocal\n").unwrap();

    // assert_cmd runs without a TTY: the gate must refuse rather than auto-push.
    repo.run(&["sync", "save work"])
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
fn sync_outside_a_git_repo_errors() {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("git-tools")
        .unwrap()
        .args(["sync", "save work", "--yes"])
        .current_dir(tmp.path())
        .assert()
        .code(1)
        .stderr(contains("not a git repo"));
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
    let manifest = repo.root.join("repos.txt");
    std::fs::write(&manifest, "repo\t\nmissing\t\n").unwrap();

    Command::cargo_bin("git-tools")
        .unwrap()
        .args([
            "status",
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
    let manifest = repo.root.join("repos.txt");
    std::fs::write(&manifest, "repo\t\n").unwrap();

    Command::cargo_bin("git-tools")
        .unwrap()
        .args([
            "status",
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
    let manifest = repo.root.join("repos.txt");
    std::fs::write(&manifest, "repo\t\n").unwrap();

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

// --- diff subrepos ---------------------------------------------------------

#[test]
fn diff_subrepos_nested_last_writes_one_tabbed_artifact() {
    let repos = NestedRepos::new(&["api", "web"]);

    repos
        .run(&["diff", "subrepos", "-l"])
        .assert()
        .success()
        .stdout(contains("diff subrepos: 2 repo(s)"))
        .stdout(contains("wrote"));

    let html = std::fs::read_to_string(repos.artifact()).unwrap();
    assert_eq!(
        html.matches(r#"<button class="tab"#).count(),
        repos.repos.len()
    );
    assert!(html.contains("api"));
    assert!(html.contains("web"));
    assert!(html.contains("diff-preview"));
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
    let manifest = repos.root.join("repos.txt");
    std::fs::write(&manifest, "api\t\nweb\t\n").unwrap();

    repos
        .run(&[
            "diff",
            "--all",
            "--repos-file",
            manifest.to_str().unwrap(),
            "--home-dir",
            repos.root.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("diff-all: 2 repo(s)"))
        .stdout(contains("wrote"));

    let artifact = repos.root.join(".artifacts/diff-preview-all.html");
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

    repo.run(&[
        "merge-diff",
        "--repo",
        repo.repo_arg(),
        "--monorepo",
        repo.monorepo_arg(),
        "--base",
        "main",
    ])
    .assert()
    .success()
    .stdout(contains("merge-diff:"))
    .stdout(contains("to merge into main"));
    assert_html(&repo.artifact("merge-diff-repo.html"), "feature");
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
