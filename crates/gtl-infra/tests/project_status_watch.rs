use std::{
    path::Path,
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use gtl_infra::project_status_watch::ProjectStatusWatch;

fn git(path: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn repository() -> anyhow::Result<tempfile::TempDir> {
    let repo = tempfile::tempdir()?;
    git(repo.path(), &["init", "-q", "-b", "main"])?;
    std::fs::write(repo.path().join(".gitignore"), "/target/\n/forced/\n")?;
    std::fs::write(repo.path().join("Cargo.lock"), "tracked\n")?;
    std::fs::create_dir(repo.path().join("forced"))?;
    std::fs::write(repo.path().join("forced/keep"), "tracked\n")?;
    git(repo.path(), &["add", ".gitignore", "Cargo.lock"])?;
    git(repo.path(), &["add", "-f", "forced/keep"])?;
    Ok(repo)
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
    let repo = repository().unwrap();
    std::fs::create_dir_all(repo.path().join("target/debug/deps")).unwrap();
    for index in 0..600 {
        std::fs::create_dir(repo.path().join(format!("target/debug/deps/{index}"))).unwrap();
    }
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, repo.path(), &AtomicBool::new(false));
    assert_eq!(watch.interval(0), Duration::from_secs(60));
    assert!(watch.registrations() < 20);
    for index in 0..500 {
        std::fs::write(
            repo.path()
                .join(format!("target/debug/deps/{index}/artifact")),
            "build",
        )
        .unwrap();
    }
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!watch.changed(0, Instant::now()));
    std::fs::write(repo.path().join("forced/keep"), "changed").unwrap();
    wait_changed(&watch);
    let _ = watch.begin_check(0);
    std::fs::write(repo.path().join("Cargo.lock"), "changed").unwrap();
    wait_changed(&watch);
    watch.detach(0);
    assert_eq!(watch.registrations(), 0);
}

#[test]
fn watch_budget_falls_back_without_retaining_partial_registrations() {
    let repo = repository().unwrap();
    for index in 0..600 {
        std::fs::create_dir(repo.path().join(format!("source-{index}"))).unwrap();
    }
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, repo.path(), &AtomicBool::new(false));
    assert_eq!(watch.interval(0), Duration::from_secs(30));
    assert_eq!(watch.registrations(), 0);
}

#[test]
fn ignore_changes_reconfigure_coverage_and_missing_repositories_can_appear() {
    let repo = repository().unwrap();
    std::fs::create_dir_all(repo.path().join("target/new")).unwrap();
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, repo.path(), &AtomicBool::new(false));
    std::fs::write(repo.path().join(".gitignore"), "").unwrap();
    wait_changed(&watch);
    assert!(watch.begin_check(0));
    watch.configure(0, repo.path(), &AtomicBool::new(false));
    std::fs::write(repo.path().join("target/new/source"), "new").unwrap();
    wait_changed(&watch);

    let empty = tempfile::tempdir().unwrap();
    watch.configure(0, empty.path(), &AtomicBool::new(false));
    let _ = watch.begin_check(0);
    git(empty.path(), &["init", "-q", "-b", "main"]).unwrap();
    wait_changed(&watch);
}

#[test]
fn linked_worktrees_watch_common_refs_and_their_own_index() {
    let repo = repository().unwrap();
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-qm",
            "initial",
        ],
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let worktree = root.path().join("linked");
    git(
        repo.path(),
        &[
            "worktree",
            "add",
            "-qb",
            "feature",
            worktree.to_str().unwrap(),
        ],
    )
    .unwrap();
    let mut watch = ProjectStatusWatch::new(1);
    watch.configure(0, &worktree, &AtomicBool::new(false));
    assert_eq!(watch.interval(0), Duration::from_secs(60));
    std::fs::write(worktree.join("Cargo.lock"), "changed").unwrap();
    wait_changed(&watch);
    let _ = watch.begin_check(0);
    git(&worktree, &["add", "Cargo.lock"]).unwrap();
    wait_changed(&watch);
    assert!(watch.begin_check(0));
    git(repo.path(), &["branch", "another"]).unwrap();
    wait_changed(&watch);
}
