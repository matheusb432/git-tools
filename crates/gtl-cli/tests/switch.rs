//! End-to-end tests for `gtl sw`: build a real temp repository with a feature branch ahead of
//! `main`, run the built binary against it, and assert exit code, stdout/stderr, and the
//! resulting ref topology. Local-only — no network.

use std::{path::Path, process};

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// Run a git command in `repo_path`, asserting success.
fn git(repo_path: &Path, args: &[&str]) -> Result<()> {
    let out = process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .context("run Git")?;
    ensure!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// `git rev-parse <rev>` (trimmed) in `repo_path`.
fn rev(repo_path: &Path, revision: &str) -> Result<String> {
    let out = process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", revision])
        .output()
        .context("resolve revision")?;
    ensure!(out.status.success(), "rev-parse {revision} failed");
    Ok(String::from_utf8(out.stdout)
        .context("revision is UTF-8")?
        .trim()
        .to_string())
}

/// The currently checked-out branch name in `repo_path`.
fn current_branch(repo_path: &Path) -> Result<String> {
    let out = process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("resolve current branch")?;
    Ok(String::from_utf8(out.stdout)
        .context("branch name is UTF-8")?
        .trim()
        .to_string())
}

/// Commit `content` to `file` in `repo_path` with message `msg`.
fn commit(repo_path: &Path, file: &str, content: &str, message: &str) -> Result<()> {
    std::fs::write(repo_path.join(file), content).context("write fixture file")?;
    git(repo_path, &["add", "."])?;
    git(repo_path, &["commit", "-qm", message])
}

/// A temp repository: `main` with one commit, then a `feature` branch +2 commits, left checked out
/// on `feature` with a clean tree. Returns (tempdir, repository path).
fn setup() -> Result<(TempDir, std::path::PathBuf)> {
    let tmp = tempfile::tempdir().context("temporary repository")?;
    let repo_path = tmp.path().to_path_buf();
    git(&repo_path, &["init", "-q", "-b", "main"])?;
    git(&repo_path, &["config", "user.name", "E2E Bot"])?;
    git(&repo_path, &["config", "user.email", "e2e@example.invalid"])?;
    git(&repo_path, &["config", "commit.gpgsign", "false"])?;
    git(&repo_path, &["config", "core.autocrlf", "false"])?;
    commit(&repo_path, "README.md", "base\n", "chore: base")?;
    git(&repo_path, &["switch", "-qc", "feature"])?;
    commit(&repo_path, "a.txt", "a\n", "feat: add a")?;
    commit(&repo_path, "b.txt", "b\n", "feat: add b")?;
    Ok((tmp, repo_path))
}

/// The built `git-tools` binary, run with cwd inside `repo_path`.
fn gtl(repo_path: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_git-tools"));
    cmd.current_dir(repo_path);
    cmd
}

#[test]
fn revert_undoes_a_rebase_and_returns_to_feature() -> Result<()> {
    let (_tmp, repo_path) = setup()?;
    let main_before = rev(&repo_path, "main")?;
    gtl(&repo_path).args(["sw", "--rebase"]).assert().success();
    // Now on main at the feature tip; revert it.
    gtl(&repo_path)
        .args(["sw", "--revert"])
        .assert()
        .success()
        .stdout(contains("reverted 'main'"));
    assert_eq!(
        rev(&repo_path, "main")?,
        main_before,
        "main reset to its pre-rebase tip"
    );
    assert_eq!(
        current_branch(&repo_path)?,
        "feature",
        "switched back to feature"
    );
    Ok(())
}
