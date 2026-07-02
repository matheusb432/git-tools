//! End-to-end tests for `gtl sw`: build a real temp repo with a feature branch ahead of
//! `main`, run the built binary against it, and assert exit code, stdout/stderr, and the
//! resulting ref topology. Local-only — no network.

use std::{path::Path, process::Command as Git};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// Run a git command in `repo`, asserting success.
fn git(repo: &Path, args: &[&str]) {
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
}

/// `git rev-parse <rev>` (trimmed) in `repo`.
fn rev(repo: &Path, r: &str) -> String {
    let out = Git::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", r])
        .output()
        .unwrap();
    assert!(out.status.success(), "rev-parse {r} failed");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The currently checked-out branch name in `repo`.
fn current_branch(repo: &Path) -> String {
    let out = Git::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Commit `content` to `file` in `repo` with message `msg`.
fn commit(repo: &Path, file: &str, content: &str, msg: &str) {
    std::fs::write(repo.join(file), content).unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", msg]);
}

/// A temp repo: `main` with one commit, then a `feature` branch +2 commits, left checked out
/// on `feature` with a clean tree. Returns (tempdir, repo path).
fn setup() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.name", "E2E Bot"]);
    git(&repo, &["config", "user.email", "e2e@example.invalid"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    commit(&repo, "README.md", "base\n", "chore: base");
    git(&repo, &["switch", "-qc", "feature"]);
    commit(&repo, "a.txt", "a\n", "feat: add a");
    commit(&repo, "b.txt", "b\n", "feat: add b");
    (tmp, repo)
}

/// The built `git-tools` binary, run with cwd inside `repo`.
fn gtl(repo: &Path) -> Command {
    let mut cmd = Command::cargo_bin("git-tools").unwrap();
    cmd.current_dir(repo);
    cmd
}

#[test]
fn switch_only_moves_head_to_main_without_moving_refs() {
    let (_tmp, repo) = setup();
    let main_before = rev(&repo, "main");
    gtl(&repo)
        .arg("sw")
        .assert()
        .success()
        .stdout(contains("switched to 'main' from 'feature'"));
    assert_eq!(
        rev(&repo, "HEAD"),
        main_before,
        "HEAD now at main's (unchanged) tip"
    );
    assert_eq!(rev(&repo, "main"), main_before, "main ref did not move");
}

#[test]
fn rebase_fast_forwards_main_onto_feature_tip() {
    let (_tmp, repo) = setup();
    let feature_tip = rev(&repo, "feature");
    gtl(&repo)
        .args(["sw", "--rebase"])
        .assert()
        .success()
        .stdout(contains("fast-forwarded main +2 commits:"))
        .stdout(contains("feat: add a"))
        .stdout(contains("feat: add b"));
    assert_eq!(
        rev(&repo, "main"),
        feature_tip,
        "main fast-forwarded to feature tip"
    );
    assert_eq!(
        rev(&repo, "HEAD"),
        feature_tip,
        "now on main at the feature tip"
    );
}

#[test]
fn refuses_to_rebase_a_dirty_tree() {
    let (_tmp, repo) = setup();
    let main_before = rev(&repo, "main");
    // uncommitted change to a tracked file
    std::fs::write(repo.join("a.txt"), "a dirty\n").unwrap();
    gtl(&repo)
        .args(["sw", "--rebase"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("working tree not clean"));
    assert_eq!(
        rev(&repo, "main"),
        main_before,
        "main unchanged after refusal"
    );
}

#[test]
fn refuses_to_rebase_when_main_has_diverged() {
    let (_tmp, repo) = setup();
    // Add a commit to main that feature lacks, then go back to feature.
    git(&repo, &["switch", "-q", "main"]);
    commit(&repo, "c.txt", "c\n", "feat: main-only");
    let main_before = rev(&repo, "main");
    git(&repo, &["switch", "-q", "feature"]);
    gtl(&repo)
        .args(["sw", "--rebase"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("has diverged"));
    assert_eq!(
        rev(&repo, "main"),
        main_before,
        "main unchanged after divergence refusal"
    );
}

#[test]
fn revert_undoes_a_rebase_and_returns_to_feature() {
    let (_tmp, repo) = setup();
    let main_before = rev(&repo, "main");
    gtl(&repo).args(["sw", "--rebase"]).assert().success();
    // Now on main at the feature tip; revert it.
    gtl(&repo)
        .args(["sw", "--revert"])
        .assert()
        .success()
        .stdout(contains("reverted 'main'"));
    assert_eq!(
        rev(&repo, "main"),
        main_before,
        "main reset to its pre-rebase tip"
    );
    assert_eq!(current_branch(&repo), "feature", "switched back to feature");
}
