#![cfg(test)]

use std::{path::Path, process::Command};

use gtl_application::{
    ports::{GitClient as _, GitEffect, GitStatusSnapshot, GitWorkingTree},
    repositories::get_recursive_repository_statuses::{self, GetRecursiveRepositoryStatuses},
};
use gtl_infra::git_client::HybridGitClient;
use gtl_models::{
    git::{BranchName, CommitCount, GitHead, GitRefName},
    paths::RepositoryRoot,
    repository::{PathCount, traversal::RepositoryTraversalScope},
};

fn repository_root(path: &Path) -> RepositoryRoot {
    RepositoryRoot::try_new(path.to_path_buf()).unwrap()
}

fn status_snapshot(root: &RepositoryRoot, expectation: &str) -> GitStatusSnapshot {
    let result = HybridGitClient.status_snapshot(root).unwrap();
    match result {
        GitEffect::Applied(snapshot) => Ok(snapshot),
        GitEffect::Rejected(detail) => Err(format!("{expectation}: {detail}")),
    }
    .unwrap()
}

#[test]
fn status_snapshot_reads_real_branch_upstream_and_changes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    init_repo(&repository);
    let origin = temporary.path().join("origin.git");
    git(
        &repository,
        &["init", "--bare", "-q", origin.to_str().unwrap()],
    );
    git(
        &repository,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&repository, &["push", "-q", "-u", "origin", "main"]);

    std::fs::write(repository.join("ahead.txt"), "ahead\n").unwrap();
    git(&repository, &["add", "ahead.txt"]);
    git(&repository, &["commit", "-qm", "ahead"]);
    std::fs::write(repository.join("tracked.txt"), "changed\n").unwrap();
    std::fs::write(repository.join("staged.txt"), "staged\n").unwrap();
    git(&repository, &["add", "staged.txt"]);
    std::fs::write(repository.join("untracked.txt"), "untracked\n").unwrap();

    let result = HybridGitClient
        .status_snapshot(&repository_root(&repository))
        .unwrap();
    let snapshot = match result {
        GitEffect::Applied(snapshot) => Some(snapshot),
        GitEffect::Rejected(_) => None,
    }
    .unwrap();

    assert_eq!(
        snapshot.head,
        GitHead::Branch(BranchName::try_new("main").unwrap())
    );
    assert_eq!(
        snapshot
            .upstream
            .map(|upstream| (upstream.reference, upstream.ahead)),
        Some((
            GitRefName::try_new("origin/main").unwrap(),
            CommitCount::new(1)
        ))
    );
    assert_eq!(snapshot.working_tree.staged, PathCount::new(1));
    assert_eq!(snapshot.working_tree.unprepared, PathCount::new(2));
    assert_eq!(
        snapshot
            .working_tree
            .files
            .iter()
            .map(|file| (file.status.as_str(), file.path.as_ref().as_path()))
            .collect::<Vec<_>>(),
        vec![
            ("A", Path::new("staged.txt")),
            ("M", Path::new("tracked.txt")),
            ("??", Path::new("untracked.txt")),
        ]
    );
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed in {}: {}",
        repository.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(repository: &Path) {
    std::fs::create_dir_all(repository).unwrap();
    git(repository, &["init", "-q", "-b", "main"]);
    git(
        repository,
        &["config", "user.email", "test@example.invalid"],
    );
    git(repository, &["config", "user.name", "Test"]);
    std::fs::write(repository.join("tracked.txt"), "tracked\n").unwrap();
    git(repository, &["add", "."]);
    git(repository, &["commit", "-qm", "initial"]);
}

fn add_submodule(repository: &Path, source: &Path, destination: &str) {
    git(
        repository,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            source.to_str().unwrap(),
            destination,
        ],
    );
    git(repository, &["commit", "-qam", "add submodule"]);
}

#[test]
fn status_queries_read_nested_submodule_repositories() {
    let temporary = tempfile::tempdir().unwrap();
    let nested_source = temporary.path().join("nested-source");
    init_repo(&nested_source);

    let submodule_source = temporary.path().join("submodule-source");
    init_repo(&submodule_source);
    add_submodule(&submodule_source, &nested_source, "ext/nested");

    let repository = temporary.path().join("repository");
    init_repo(&repository);
    add_submodule(&repository, &submodule_source, "lib/submodule");
    git(
        &repository,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "--init",
            "--recursive",
        ],
    );

    let root = repository_root(&repository);
    let snapshot = status_snapshot(&root, "inspect a repository with nested submodules");

    assert_eq!(
        snapshot.head,
        GitHead::Branch(BranchName::try_new("main").unwrap())
    );
    assert_eq!(snapshot.upstream, None);
    assert_eq!(snapshot.working_tree, GitWorkingTree::default());

    let recursive = get_recursive_repository_statuses::execute(
        GetRecursiveRepositoryStatuses {
            root: repository.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        },
        &HybridGitClient,
    )
    .unwrap();
    assert_eq!(recursive.len(), 3);
    assert!(
        recursive
            .iter()
            .all(|status| status.detail() != "status-unavailable")
    );

    let working_tree = HybridGitClient.working_tree(&root).unwrap();

    assert_eq!(working_tree, GitEffect::Applied(GitWorkingTree::default()));

    std::fs::write(
        repository.join("lib/submodule/ext/nested/tracked.txt"),
        "changed\n",
    )
    .unwrap();
    let dirty_snapshot = status_snapshot(&root, "inspect a dirty nested submodule");

    assert_eq!(dirty_snapshot.working_tree.files.len(), 1);
    assert_eq!(dirty_snapshot.working_tree.files[0].status, "M");
    assert_eq!(
        dirty_snapshot.working_tree.files[0].path.as_ref().as_path(),
        Path::new("lib/submodule")
    );
    assert_eq!(dirty_snapshot.working_tree.unprepared, PathCount::new(1));

    git(
        &repository,
        &["config", "submodule.lib/submodule.ignore", "dirty"],
    );
    let ignored_dirty = status_snapshot(&root, "honor dirty-submodule configuration");
    assert_eq!(ignored_dirty.working_tree, GitWorkingTree::default());

    git(
        &repository,
        &["config", "submodule.lib/submodule.ignore", "untracked"],
    );
    let tracked_change = status_snapshot(&root, "report tracked submodule changes");
    assert_eq!(tracked_change.working_tree.files.len(), 1);

    std::fs::write(
        repository.join("lib/submodule/ext/nested/tracked.txt"),
        "tracked\n",
    )
    .unwrap();
    std::fs::write(
        repository.join("lib/submodule/ext/nested/untracked.txt"),
        "untracked\n",
    )
    .unwrap();
    let ignored_untracked = status_snapshot(&root, "honor untracked-submodule configuration");
    assert_eq!(ignored_untracked.working_tree, GitWorkingTree::default());

    git(
        &repository,
        &["config", "submodule.lib/submodule.ignore", "none"],
    );
    let visible_untracked = status_snapshot(&root, "include configured nested untracked files");
    assert_eq!(visible_untracked.working_tree.files.len(), 1);
}
