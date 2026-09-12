#![cfg(test)]

use std::{path::Path, process::Command};

use gtl_application::ports::GitClient as _;
use gtl_infra::git_client::HybridGitClient;
use gtl_models::{git::GitRevision, paths::RepositoryRoot};

fn repository_root(path: &Path) -> RepositoryRoot {
    RepositoryRoot::try_new(path.to_path_buf()).unwrap()
}

fn git(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn init_repo(repo_path: &Path) {
    git(repo_path, &["init", "-q", "-b", "main"]);
    git(repo_path, &["config", "user.email", "test@example.invalid"]);
    git(repo_path, &["config", "user.name", "Test"]);
    std::fs::write(repo_path.join("tracked.txt"), "initial\n").unwrap();
    git(repo_path, &["add", "."]);
    git(repo_path, &["commit", "-qm", "initial"]);
}

#[test]
fn real_git_resolves_an_annotated_tag_to_its_commit_id() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    git(repo.path(), &["tag", "-am", "release", "v1.0.0"]);

    let revision = GitRevision::try_new("v1.0.0").unwrap();
    let repo_root = repository_root(repo.path());
    let id = HybridGitClient
        .resolve_commit_id(&repo_root, &revision)
        .unwrap();

    assert_eq!(id.as_ref(), git(repo.path(), &["rev-parse", "HEAD"]));
    assert_ne!(id.as_ref(), git(repo.path(), &["rev-parse", "v1.0.0"]));
}
