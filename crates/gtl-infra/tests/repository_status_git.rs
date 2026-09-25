#![cfg(test)]

use std::path::{Path, PathBuf};

use gtl_application::{
    ports::{GitClient as _, GitEffect, GitStatusSnapshot, GitWorkingTree},
    repositories::get_recursive_repository_statuses::{self, GetRecursiveRepositoryStatuses},
};
use gtl_infra::{git_client::HybridGitClient, testing::TestRepository};
use gtl_models::{
    git::{BranchName, CommitCount, GitHead, GitRefName},
    paths::RepositoryRoot,
    repository::{PathCount, traversal::RepositoryTraversalScope},
};

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
    let repository = committed_repository(&temporary.path().join("repository"));
    repository.add_bare_origin(temporary.path().join("origin.git"));

    repository.write("ahead.txt", "ahead\n");
    repository.commit_all("ahead");
    repository.write("tracked.txt", "changed\n");
    repository.write("staged.txt", "staged\n");
    repository.git(&["add", "staged.txt"]);
    repository.write("untracked.txt", "untracked\n");

    let result = HybridGitClient.status_snapshot(&repository.root()).unwrap();
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

#[test]
fn recursive_status_reports_a_linked_worktree_root_and_its_submodules() {
    let temporary = tempfile::tempdir().unwrap();
    let (_repository, worktree) = repository_with_feature_worktree(temporary.path());
    TestRepository::open(&worktree).git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "update",
        "--init",
    ]);

    assert_eq!(
        recursive_branch_labels(&worktree),
        [
            ("feature".to_owned(), Some("feature".to_owned())),
            ("lib/submodule".to_owned(), Some("detached".to_owned()))
        ]
    );
}

#[test]
fn recursive_status_reports_submodule_checkouts_that_are_linked_worktrees() {
    let temporary = tempfile::tempdir().unwrap();
    let (repository, worktree) = repository_with_feature_worktree(temporary.path());
    repository.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "update",
        "--init",
    ]);
    TestRepository::open(repository.path().join("lib/submodule")).git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        worktree.join("lib/submodule").to_str().unwrap(),
    ]);

    assert_eq!(
        recursive_branch_labels(&worktree),
        [
            ("feature".to_owned(), Some("feature".to_owned())),
            ("lib/submodule".to_owned(), Some("detached".to_owned()))
        ]
    );
}

/// Returns a repository that added `lib/submodule` and the path of its `feature` linked worktree,
/// where the submodule is not checked out yet.
fn repository_with_feature_worktree(directory: &Path) -> (TestRepository, PathBuf) {
    let submodule_source = committed_repository(&directory.join("submodule-source"));
    let repository = committed_repository(&directory.join("repository"));
    add_submodule(&repository, &submodule_source, "lib/submodule");
    let worktree = repository.path().join(".worktrees/feature");
    repository.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "feature",
        worktree.to_str().unwrap(),
    ]);
    (repository, worktree)
}

fn recursive_branch_labels(root: &Path) -> Vec<(String, Option<String>)> {
    get_recursive_repository_statuses::execute(
        GetRecursiveRepositoryStatuses {
            root: root.canonicalize().unwrap(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        },
        &HybridGitClient,
    )
    .unwrap()
    .iter()
    .map(|result| {
        (
            result.name().to_string(),
            result.branch_label().map(str::to_owned),
        )
    })
    .collect()
}

/// Initializes a repository at `path` with `tracked.txt` committed as `tracked`.
fn committed_repository(path: &Path) -> TestRepository {
    let repository = TestRepository::init(path);
    repository.write("tracked.txt", "tracked\n");
    repository.commit_all("initial");
    repository
}

fn add_submodule(repository: &TestRepository, source: &TestRepository, destination: &str) {
    repository.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "add",
        source.path().to_str().unwrap(),
        destination,
    ]);
    repository.commit_all("add submodule");
}

#[test]
fn status_queries_read_nested_submodule_repositories() {
    let temporary = tempfile::tempdir().unwrap();
    let nested_source = committed_repository(&temporary.path().join("nested-source"));

    let submodule_source = committed_repository(&temporary.path().join("submodule-source"));
    add_submodule(&submodule_source, &nested_source, "ext/nested");

    let repository = committed_repository(&temporary.path().join("repository"));
    add_submodule(&repository, &submodule_source, "lib/submodule");
    repository.git(&[
        "-c",
        "protocol.file.allow=always",
        "submodule",
        "update",
        "--init",
        "--recursive",
    ]);

    let root = repository.root();
    let snapshot = status_snapshot(&root, "inspect a repository with nested submodules");

    assert_eq!(
        snapshot.head,
        GitHead::Branch(BranchName::try_new("main").unwrap())
    );
    assert_eq!(snapshot.upstream, None);
    assert_eq!(snapshot.working_tree, GitWorkingTree::default());

    let recursive = get_recursive_repository_statuses::execute(
        GetRecursiveRepositoryStatuses {
            root: repository.path().to_path_buf(),
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

    repository.write("lib/submodule/ext/nested/tracked.txt", "changed\n");
    let dirty_snapshot = status_snapshot(&root, "inspect a dirty nested submodule");

    assert_eq!(dirty_snapshot.working_tree.files.len(), 1);
    assert_eq!(dirty_snapshot.working_tree.files[0].status, "M");
    assert_eq!(
        dirty_snapshot.working_tree.files[0].path.as_ref().as_path(),
        Path::new("lib/submodule")
    );
    assert_eq!(dirty_snapshot.working_tree.unprepared, PathCount::new(1));

    repository.git(&["config", "submodule.lib/submodule.ignore", "dirty"]);
    let ignored_dirty = status_snapshot(&root, "honor dirty-submodule configuration");
    assert_eq!(ignored_dirty.working_tree, GitWorkingTree::default());

    repository.git(&["config", "submodule.lib/submodule.ignore", "untracked"]);
    let tracked_change = status_snapshot(&root, "report tracked submodule changes");
    assert_eq!(tracked_change.working_tree.files.len(), 1);

    repository.write("lib/submodule/ext/nested/tracked.txt", "tracked\n");
    repository.write("lib/submodule/ext/nested/untracked.txt", "untracked\n");
    let ignored_untracked = status_snapshot(&root, "honor untracked-submodule configuration");
    assert_eq!(ignored_untracked.working_tree, GitWorkingTree::default());

    repository.git(&["config", "submodule.lib/submodule.ignore", "none"]);
    let visible_untracked = status_snapshot(&root, "include configured nested untracked files");
    assert_eq!(visible_untracked.working_tree.files.len(), 1);
}
