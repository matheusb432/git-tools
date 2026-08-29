#![cfg(test)]

use std::{path::Path, process::Command};

use gtl_application::{
    ports::GitClient as _,
    repositories::{
        CommitProgress,
        apply_commit::{self, ApplyCommit},
        plan_commit::CommitTarget,
    },
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::{
    git::{BranchName, GitRevision},
    paths::{ProjectName, RepositoryRoot},
    repository::PendingChanges,
};

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

fn init_dirty_repo(repo_path: &Path) {
    git(repo_path, &["init", "-q", "-b", "main"]);
    git(repo_path, &["config", "user.email", "test@example.invalid"]);
    git(repo_path, &["config", "user.name", "Test"]);
    std::fs::write(repo_path.join("tracked.txt"), "initial\n").unwrap();
    git(repo_path, &["add", "."]);
    git(repo_path, &["commit", "-qm", "initial"]);
    install_stderr_post_commit_hook(repo_path);
    std::fs::write(repo_path.join("tracked.txt"), "changed\n").unwrap();
}

fn install_stderr_post_commit_hook(repo_path: &Path) {
    let hook = repo_path.join(".git/hooks/post-commit");
    std::fs::write(&hook, "#!/bin/sh\nprintf '%s\\n' '[hook deadbeef]' >&2\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        let mut permissions = std::fs::metadata(&hook).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&hook, permissions).unwrap();
    }
}

#[test]
fn real_git_apply_commit_ignores_post_commit_stderr() {
    let apply_repo = tempfile::tempdir().unwrap();
    init_dirty_repo(apply_repo.path());

    let applied = apply_commit::execute(
        ApplyCommit {
            target: CommitTarget {
                name: ProjectName::try_from("apply").unwrap(),
                top: repository_root(apply_repo.path()),
                branch: BranchName::try_new("main").unwrap(),
                pending: PendingChanges::default(),
            },
            message: "apply change".into(),
        },
        &HybridGitClient,
    )
    .unwrap();
    let apply_identity = match applied.progress {
        CommitProgress::Created { id } => Some(id),
        _ => None,
    }
    .unwrap();
    assert_eq!(
        apply_identity.as_ref(),
        git(apply_repo.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn real_git_resolves_an_annotated_tag_to_its_commit_id() {
    let repo = tempfile::tempdir().unwrap();
    init_dirty_repo(repo.path());
    git(repo.path(), &["tag", "-am", "release", "v1.0.0"]);

    let revision = GitRevision::try_new("v1.0.0").unwrap();
    let repo_root = repository_root(repo.path());
    let id = HybridGitClient
        .resolve_commit_id(&repo_root, &revision)
        .unwrap();

    assert_eq!(id.as_ref(), git(repo.path(), &["rev-parse", "HEAD"]));
    assert_ne!(id.as_ref(), git(repo.path(), &["rev-parse", "v1.0.0"]));
}
