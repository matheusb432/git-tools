use std::{path::Path, process::Command};

use gtl_application::repository_sync::{
    CommitProgress,
    apply_commit::{self, ApplyCommit},
    plan_commit::CommitTarget,
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::repository::PendingChanges;

fn git(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .expect("git starts");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_string()
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
                name: "apply".into(),
                top: apply_repo.path().into(),
                branch: "main".into(),
                pending: PendingChanges::default(),
            },
            message: "apply change".into(),
        },
        &HybridGitClient,
    )
    .expect("current-repository commit succeeds through real Git");
    let apply_identity = match applied.progress {
        CommitProgress::Created { identity } => identity,
        progress => panic!("expected a created commit, got {progress:?}"),
    };
    assert_eq!(
        apply_identity.as_deref(),
        Some(git(apply_repo.path(), &["rev-parse", "--short", "HEAD"]).as_str())
    );
}
