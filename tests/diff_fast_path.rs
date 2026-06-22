use std::process::Command;

// Builds a tiny repo with two commits, runs `gtl diff HEAD~1..HEAD` twice
// against a shared store, and asserts the second run reports reuse.
#[test]
fn second_identical_range_run_reuses_artifact() {
    let repo = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let g = |args: &[&str]| {
        assert!(Command::new("git").arg("-C").arg(repo.path()).args(args).status().unwrap().success());
    };
    g(&["init", "-q"]);
    g(&["config", "user.email", "t@t"]);
    g(&["config", "user.name", "t"]);
    std::fs::write(repo.path().join("a.txt"), "a\n").unwrap();
    g(&["add", "."]);
    g(&["commit", "-qm", "base"]);
    g(&["branch", "-M", "main"]);
    std::fs::write(repo.path().join("a.txt"), "a\nb\n").unwrap();
    g(&["add", "."]);
    g(&["commit", "-qm", "change"]);

    let run = || {
        assert_cmd::Command::cargo_bin("git-tools")
            .unwrap()
            .current_dir(repo.path())
            .env("GIT_TOOLS_DATA_DIR", store.path())
            .env("GIT_TOOLS_NO_OPEN", "1")
            .args(["diff", "HEAD~1..HEAD"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    let _first = run();
    let second = String::from_utf8(run()).unwrap();
    assert!(second.contains("reusing"), "second run should reuse: {second}");
}
