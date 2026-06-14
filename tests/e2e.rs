//! End-to-end tests: build real temporary git repos, run the built binary, and assert on
//! exit codes, stdout/stderr, and the HTML artifacts written under `<monorepo>/.artifacts`.
//!
//! These replace the PowerShell conformance harness. That harness froze the legacy Node/PS
//! originals as goldens to prove the Rust port matched them; the port is done and the diff
//! surface has since moved on (lean `diff` + `diff-subrepos`, GTL-0002), so these pin
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

// --- diff-subrepos ---------------------------------------------------------

#[test]
fn diff_subrepos_writes_under_monorepo_not_repo() {
    let repo = Repo::new();
    let base = repo.commit("a.txt", "base\n", "chore: base");
    repo.commit("a.txt", "base\nsub\n", "feat: sub work");

    repo.run(&[
        "diff-subrepos",
        "--repo",
        repo.repo_arg(),
        "--monorepo",
        repo.monorepo_arg(),
        "--base",
        &base,
    ])
    .assert()
    .success()
    .stdout(contains("diff-preview:"));
    assert_html(&repo.artifact("diff-preview-repo.html"), "main");
    assert!(!repo.repo.join(".artifacts/diff-preview-repo.html").exists());
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
