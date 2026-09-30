use std::{path::Path, process};

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;

mod common;

fn git(repository: &Path, arguments: &[&str]) -> Result<()> {
    let output = process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .context("run Git")?;
    ensure!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn repository_with_changed_last_commit() -> Result<tempfile::TempDir> {
    let repository = tempfile::tempdir().context("create temporary repository")?;
    git(repository.path(), &["init", "-q", "-b", "main"])?;
    git(repository.path(), &["config", "user.name", "E2E Bot"])?;
    git(
        repository.path(),
        &["config", "user.email", "e2e@example.invalid"],
    )?;
    git(
        repository.path(),
        &["commit", "-q", "--allow-empty", "-m", "chore: base"],
    )?;
    std::fs::write(repository.path().join("changed.txt"), "changed\n")
        .context("write changed file")?;
    git(repository.path(), &["add", "changed.txt"])?;
    git(repository.path(), &["commit", "-qm", "feat: change file"])?;
    Ok(repository)
}

#[test]
fn caret_bang_target_diffs_only_that_commit() -> Result<()> {
    let repository = repository_with_changed_last_commit()?;
    let _server = common::ServerHarness::start(None, None)?;

    let output = Command::new(common::cli_binary())
        .current_dir(repository.path())
        .args(["diff", "HEAD^!", "--raw"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output)?;
    let artifact = output
        .lines()
        .find_map(|line| line.strip_prefix(if cfg!(windows) { "file:///" } else { "file://" }))
        .context("single-commit render must return an artifact")?;
    let rendered = std::fs::read_to_string(artifact)?;
    ensure!(
        rendered.contains("changed.txt"),
        "single-commit diff must show the file that commit changed"
    );
    Ok(())
}
