//! End-to-end tests for `gtl prune`: build a real temp repo with merged and unmerged
//! branches, run the built binary against it, and assert exit code, stdout/stderr, and the
//! surviving branch set. Local-only — no network.

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

/// True when `branch` still exists locally in `repo`.
fn branch_exists(repo: &Path, branch: &str) -> bool {
    Git::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", &format!("refs/heads/{branch}")])
        .output()
        .unwrap()
        .status
        .success()
}

/// A temp repo on `main` with: `feat/merged` (fast-forward-merged into main) and
/// `feat/wip` (one commit ahead, never merged). Left checked out on `main`, clean tree.
fn setup() -> (TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().to_path_buf();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.name", "E2E Bot"]);
    git(&repo, &["config", "user.email", "e2e@example.invalid"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    git(
        &repo,
        &["commit", "-q", "--allow-empty", "-m", "chore: base"],
    );

    git(&repo, &["switch", "-qc", "feat/merged"]);
    git(
        &repo,
        &["commit", "-q", "--allow-empty", "-m", "feat: done"],
    );
    git(&repo, &["switch", "-q", "main"]);
    git(&repo, &["merge", "-q", "--ff-only", "feat/merged"]);

    git(&repo, &["switch", "-qc", "feat/wip"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "wip"]);
    git(&repo, &["switch", "-q", "main"]);
    (tmp, repo)
}

/// The built `git-tools` binary, run with cwd inside `repo`.
fn gtl(repo: &Path) -> Command {
    let mut cmd = Command::cargo_bin("git-tools").unwrap();
    cmd.current_dir(repo);
    cmd
}

#[test]
fn prune_partial_failure_reports_recovery_and_preserves_blocked_branch() {
    let (_tmp, repo) = setup();
    git(&repo, &["branch", "feat/blocked"]);
    let linked_worktree = repo.join("linked-worktree");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            linked_worktree.to_str().unwrap(),
            "feat/blocked",
        ],
    );

    gtl(&repo)
        .args(["prune", "-y"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("will delete 2 branch(es) merged into 'main':"))
        .stderr(contains("prune: deleted 1 branch."))
        .stderr(contains("recover: git branch feat/merged"))
        .stderr(contains("failed: feat/blocked"));

    assert!(
        !branch_exists(&repo, "feat/merged"),
        "deletable merged branch removed"
    );
    assert!(
        branch_exists(&repo, "feat/blocked"),
        "checked-out merged branch preserved"
    );
    assert!(branch_exists(&repo, "feat/wip"), "unmerged branch kept");
}
