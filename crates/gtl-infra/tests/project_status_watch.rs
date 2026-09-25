use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use gtl_infra::{project_status_watch::ProjectStatusWatch, testing::TestRepository};

/// Stages a lockfile and a force-added file under an ignored directory, without committing.
fn repository_with_tracked_ignored_paths() -> TestRepository {
    let repository = TestRepository::new();
    repository.write(".gitignore", "/target/\n/forced/\n");
    repository.write("Cargo.lock", "tracked\n");
    repository.write("forced/keep", "tracked\n");
    repository.git(&["add", ".gitignore", "Cargo.lock"]);
    repository.git(&["add", "-f", "forced/keep"]);
    repository
}

fn wait_changed(watch: &ProjectStatusWatch) {
    let start = Instant::now();
    while !watch.changed(0, Instant::now()) {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "missing filesystem event"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn watch_prunes_build_trees_and_observes_tracked_ignored_paths_and_lockfiles() {
    let repository = repository_with_tracked_ignored_paths();
    std::fs::create_dir_all(repository.path().join("target/debug/deps")).unwrap();
    for index in 0..600 {
        std::fs::create_dir(repository.path().join(format!("target/debug/deps/{index}"))).unwrap();
    }
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, repository.path(), &AtomicBool::new(false));
    assert_eq!(watch.interval(0), Duration::from_secs(60));
    assert!(watch.registrations() < 20);
    for index in 0..500 {
        repository.write(&format!("target/debug/deps/{index}/artifact"), "build");
    }
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!watch.changed(0, Instant::now()));
    repository.write("forced/keep", "changed");
    wait_changed(&watch);
    let _ = watch.begin_check(0);
    repository.write("Cargo.lock", "changed");
    wait_changed(&watch);
    watch.detach(0);
    assert_eq!(watch.registrations(), 0);
}

#[test]
fn watch_budget_falls_back_without_retaining_partial_registrations() {
    let repository = repository_with_tracked_ignored_paths();
    for index in 0..600 {
        std::fs::create_dir(repository.path().join(format!("source-{index}"))).unwrap();
    }
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, repository.path(), &AtomicBool::new(false));
    assert_eq!(watch.interval(0), Duration::from_secs(30));
    assert_eq!(watch.registrations(), 0);
}

#[test]
fn ignore_changes_reconfigure_coverage_and_missing_repositories_can_appear() {
    let repository = repository_with_tracked_ignored_paths();
    std::fs::create_dir_all(repository.path().join("target/new")).unwrap();
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, repository.path(), &AtomicBool::new(false));
    repository.write(".gitignore", "");
    wait_changed(&watch);
    assert!(watch.begin_check(0));
    watch.configure(0, repository.path(), &AtomicBool::new(false));
    repository.write("target/new/source", "new");
    wait_changed(&watch);

    let empty = tempfile::tempdir().unwrap();
    watch.configure(0, empty.path(), &AtomicBool::new(false));
    let _ = watch.begin_check(0);
    let _appeared_repository = TestRepository::init(empty.path());
    wait_changed(&watch);
}

#[test]
fn linked_worktrees_watch_common_refs_and_their_own_index() {
    let repository = repository_with_tracked_ignored_paths();
    repository.commit_all("initial");
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("linked");
    repository.git(&[
        "worktree",
        "add",
        "-qb",
        "feature",
        worktree.to_str().unwrap(),
    ]);
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, &worktree, &AtomicBool::new(false));
    assert_eq!(watch.interval(0), Duration::from_secs(60));
    std::fs::write(worktree.join("Cargo.lock"), "changed").unwrap();
    wait_changed(&watch);
    let _ = watch.begin_check(0);
    TestRepository::open(&worktree).git(&["add", "Cargo.lock"]);
    wait_changed(&watch);
    assert!(watch.begin_check(0));
    repository.git(&["branch", "another"]);
    wait_changed(&watch);
}
