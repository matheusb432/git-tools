use std::{fs, path::Path, process::Command as ProcessCommand};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn run_git(repository_directory: &Path, arguments: &[&str]) {
    let status = ProcessCommand::new("git")
        .current_dir(repository_directory)
        .args(arguments)
        .status()
        .expect("git starts");
    assert!(status.success(), "git {arguments:?} failed");
}

fn repository_with_markdown_staged(contents_staged: &str, contents_worktree: &str) -> TempDir {
    let repository = TempDir::new().expect("temporary repository");
    run_git(repository.path(), &["init", "--quiet"]);
    fs::write(repository.path().join("README.md"), contents_staged).expect("write staged Markdown");
    run_git(repository.path(), &["add", "README.md"]);
    fs::write(repository.path().join("README.md"), contents_worktree)
        .expect("write working-tree Markdown");
    repository
}

fn repository_with_file_staged(
    path: &str,
    contents_staged: &str,
    contents_worktree: &str,
) -> TempDir {
    let repository = TempDir::new().expect("temporary repository");
    run_git(repository.path(), &["init", "--quiet"]);
    let file = repository.path().join(path);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).expect("create staged file parent");
    }
    fs::write(&file, contents_staged).expect("write staged file");
    run_git(repository.path(), &["add", path]);
    fs::write(file, contents_worktree).expect("write working-tree file");
    repository
}

#[test]
fn pre_commit_accepts_an_empty_index() {
    let repository = TempDir::new().expect("temporary repository");
    run_git(repository.path(), &["init", "--quiet"]);

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "RESULT scope=pre-commit status=PASS",
        ));
}

#[test]
fn pre_commit_checks_staged_content_instead_of_worktree() {
    let repository = repository_with_markdown_staged("# Title\n", "#  Title\n");

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .success();
}

#[test]
fn pre_commit_rejects_unformatted_staged_content() {
    let repository = repository_with_markdown_staged("#  Title\n", "# Title\n");

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .failure();
}

#[test]
fn pre_commit_rejects_unformatted_staged_rust() {
    let repository = repository_with_file_staged("main.rs", "fn main(){}\n", "fn main() {}\n");
    fs::write(
        repository.path().join(".rustfmt-nightly"),
        include_str!("../../.rustfmt-nightly"),
    )
    .expect("write pinned nightly");
    fs::write(
        repository.path().join("rustfmt.toml"),
        include_str!("../../rustfmt.toml"),
    )
    .expect("write rustfmt configuration");
    run_git(
        repository.path(),
        &["add", ".rustfmt-nightly", "rustfmt.toml"],
    );

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .failure();
}

#[test]
fn pre_commit_rejects_unformatted_staged_toml() {
    let repository =
        repository_with_file_staged("config.toml", "values=[1,2]\n", "values = [1, 2]\n");

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .failure();
}

#[test]
fn pre_commit_handles_renamed_paths_with_spaces() {
    let repository = TempDir::new().expect("temporary repository");
    run_git(repository.path(), &["init", "--quiet"]);
    fs::write(repository.path().join("README.md"), "# Title\n").expect("write baseline file");
    run_git(repository.path(), &["add", "README.md"]);
    run_git(
        repository.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--quiet",
            "-m",
            "baseline",
        ],
    );
    fs::create_dir(repository.path().join("docs")).expect("create docs directory");
    run_git(repository.path(), &["mv", "README.md", "docs/Bad Name.md"]);
    fs::write(repository.path().join("docs/Bad Name.md"), "#  Title\n")
        .expect("write staged rename");
    run_git(repository.path(), &["add", "docs/Bad Name.md"]);
    fs::write(repository.path().join("docs/Bad Name.md"), "# Title\n")
        .expect("write working-tree rename");

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .failure();
}

#[test]
fn pre_commit_accepts_a_deleted_file() {
    let repository = TempDir::new().expect("temporary repository");
    run_git(repository.path(), &["init", "--quiet"]);
    fs::write(repository.path().join("README.md"), "# Title\n").expect("write baseline file");
    run_git(repository.path(), &["add", "README.md"]);
    run_git(
        repository.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--quiet",
            "-m",
            "baseline",
        ],
    );
    run_git(repository.path(), &["rm", "--quiet", "README.md"]);

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .success();
}

#[cfg(unix)]
#[test]
fn pre_commit_reports_a_non_utf8_staged_path() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let repository = TempDir::new().expect("temporary repository");
    run_git(repository.path(), &["init", "--quiet"]);
    let file_name = OsString::from_vec(b"invalid-\xff.txt".to_vec());
    fs::write(repository.path().join(&file_name), "content\n").expect("write non-UTF-8 path");
    let status = ProcessCommand::new("git")
        .current_dir(repository.path())
        .arg("add")
        .arg(&file_name)
        .status()
        .expect("git starts");
    assert!(status.success(), "git add failed");

    Command::cargo_bin("xtask")
        .unwrap()
        .current_dir(repository.path())
        .arg("pre-commit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("staged path is not UTF-8"));
}
