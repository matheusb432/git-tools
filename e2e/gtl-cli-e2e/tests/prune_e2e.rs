#![cfg(test)]

//! End-to-end tests for `gtl prune`: build a real temp repository with merged and unmerged
//! branches, run the built binary against it, and assert exit code, stdout/stderr, and the
//! surviving branch set. Local-only — no network.

use std::{path::Path, process};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

mod support;
use support::workspace_bin;

/// Run a git command in `repo_path`, asserting success.
fn git(repo_path: &Path, args: &[&str]) {
    let out = process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// True when `branch` still exists locally in `repo_path`.
fn branch_exists(repo_path: &Path, branch: &str) -> bool {
    process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .output()
        .unwrap()
        .status
        .success()
}

/// A temp repository on `main` with: `feat/merged` (fast-forward-merged into main) and
/// `feat/wip` (one commit ahead, never merged). Left checked out on `main`, clean tree.
fn setup() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo_path = tmp.path().to_path_buf();
    git(&repo_path, &["init", "-q", "-b", "main"]);
    git(&repo_path, &["config", "user.name", "E2E Bot"]);
    git(&repo_path, &["config", "user.email", "e2e@example.invalid"]);
    git(&repo_path, &["config", "commit.gpgsign", "false"]);
    git(
        &repo_path,
        &["commit", "-q", "--allow-empty", "-m", "chore: base"],
    );

    git(&repo_path, &["switch", "-qc", "feat/merged"]);
    git(
        &repo_path,
        &["commit", "-q", "--allow-empty", "-m", "feat: done"],
    );
    git(&repo_path, &["switch", "-q", "main"]);
    git(&repo_path, &["merge", "-q", "--ff-only", "feat/merged"]);

    git(&repo_path, &["switch", "-qc", "feat/wip"]);
    git(&repo_path, &["commit", "-q", "--allow-empty", "-m", "wip"]);
    git(&repo_path, &["switch", "-q", "main"]);
    (tmp, repo_path)
}

/// The built `git-tools` binary, run with cwd inside `repo_path`.
fn gtl(repo_path: &Path) -> Command {
    let mut cmd = Command::new(workspace_bin("git-tools"));
    cmd.current_dir(repo_path);
    cmd
}

#[test]
fn prune_partial_failure_reports_recovery_and_preserves_blocked_branch() {
    let (_tmp, repo_path) = setup();
    git(&repo_path, &["branch", "feat/blocked"]);
    let linked_worktree = repo_path.join("linked-worktree");
    git(
        &repo_path,
        &[
            "worktree",
            "add",
            "-q",
            linked_worktree.to_str().unwrap(),
            "feat/blocked",
        ],
    );

    gtl(&repo_path)
        .args(["prune", "-y"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("will delete 2 branch(es) merged into 'main':"))
        .stderr(contains("prune: deleted 1 branch."))
        .stderr(contains("recover: git branch feat/merged"))
        .stderr(contains("failed: feat/blocked"));

    assert!(
        !branch_exists(&repo_path, "feat/merged"),
        "deletable merged branch removed"
    );
    assert!(
        branch_exists(&repo_path, "feat/blocked"),
        "checked-out merged branch preserved"
    );
    assert!(
        branch_exists(&repo_path, "feat/wip"),
        "unmerged branch kept"
    );
}
