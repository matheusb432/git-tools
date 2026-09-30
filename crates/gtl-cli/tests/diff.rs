use std::{path::Path, process};

use anyhow::{Context as _, Result, ensure};
use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

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

fn repository_with_changed_last_commit() -> Result<TempDir> {
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
fn default_diff_attempts_server_owned_viewer_without_display_environment() -> Result<()> {
    let repository = repository_with_changed_last_commit()?;
    let viewer_stub = tempfile::tempdir().context("create viewer stub directory")?;
    std::fs::write(
        viewer_stub
            .path()
            .join(format!("gtl-viewer{}", std::env::consts::EXE_SUFFIX)),
        "",
    )
    .context("write non-executable viewer stub")?;
    let inherited_path = std::env::var_os("PATH").context("missing executable path")?;
    let executable_path = std::env::join_paths(
        std::iter::once(viewer_stub.path().to_path_buf())
            .chain(std::env::split_paths(&inherited_path)),
    )
    .context("build isolated executable path")?;
    unsafe {
        std::env::set_var("PATH", executable_path);
        std::env::remove_var("DISPLAY");
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("GIT_TOOLS_NO_OPEN");
    }
    let _server = common::ServerHarness::start(None, None)?;

    Command::new(common::cli_binary())
        .current_dir(repository.path())
        .args(["d", "-l"])
        .assert()
        .success()
        .stderr(contains("diff: viewer unavailable"))
        .stdout(contains("file://"));

    Command::new(common::cli_binary())
        .current_dir(repository.path())
        .args(["d", "-l", "--raw"])
        .assert()
        .success()
        .stderr("")
        .stdout(contains("file://"));

    std::fs::write(
        repository.path().join("untracked.txt"),
        "cli-untracked-marker\n",
    )?;
    let index = std::fs::read(repository.path().join(".git/index"))?;
    let output = Command::new(common::cli_binary())
        .current_dir(repository.path())
        .args(["diff", "HEAD", "--raw"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output)?;
    let artifact = output
        .lines()
        .find(|line| line.starts_with("file://"))
        .context("working-tree render must return an artifact")?;
    let artifact = artifact
        .strip_prefix(if cfg!(windows) { "file:///" } else { "file://" })
        .context("artifact must use the platform's local file URL shape")?;
    ensure!(
        std::fs::read_to_string(artifact)?.contains("cli-untracked-marker"),
        "CLI working-tree comparison omitted untracked content"
    );
    ensure!(
        std::fs::read(repository.path().join(".git/index"))? == index,
        "CLI diff changed the real index"
    );
    Ok(())
}
