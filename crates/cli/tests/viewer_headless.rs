//! Ported from the retired `spec/viewer_headless_spec.sh`. Contract: `viewer=app` (the default)
//! is NON-FATAL when there is no display. With `DISPLAY` and `WAYLAND_DISPLAY` unset, `gtl diff`
//! must exit 0, print "wrote" on stdout (artifact written to the central store), and leave the
//! repo working tree clean (no `.artifacts/` or other junk).

use std::{fs, process::Command};

mod common;

/// A minimal two-commit repo so `HEAD~1..HEAD` has real content.
fn two_commit_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let g = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    g(&["init", "-q"]);
    g(&["config", "user.email", "test@example.com"]);
    g(&["config", "user.name", "Test"]);
    fs::write(repo.path().join("file.txt"), "hello\n").unwrap();
    g(&["add", "file.txt"]);
    g(&["commit", "-q", "-m", "initial"]);
    fs::write(repo.path().join("file.txt"), "hello\nworld\n").unwrap();
    g(&["add", "file.txt"]);
    g(&["commit", "-q", "-m", "second"]);
    repo
}

#[test]
fn diff_with_no_display_writes_to_store_and_keeps_repo_clean() {
    let repo = two_commit_repo();
    let store = tempfile::tempdir().unwrap();
    common::ensure_daemon_built();

    assert_cmd::Command::cargo_bin("git-tools")
        .unwrap()
        .current_dir(repo.path())
        .args(["diff", "-l", "1"])
        .env("GIT_TOOLS_DATA_DIR", store.path())
        .env("GIT_TOOLS_DAEMON_IDLE_SECS", "2")
        // The no-display fallback lands on the browser path, which honors this guard; nothing
        // opens.
        .env("GIT_TOOLS_NO_OPEN", "1")
        // Force viewer=app (the default) regardless of the user's local config.
        .env("GIT_TOOLS_CONFIG", "/dev/null")
        // Remove the display vars so viewer=app degrades to the (non-fatal) browser path.
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .assert()
        .success()
        .stdout(predicates::str::contains("wrote"));

    assert!(
        !repo.path().join(".artifacts").exists(),
        "viewer=app headless must not pollute the repo with .artifacts/"
    );

    let porcelain = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        porcelain.stdout.is_empty(),
        "working tree must stay clean, got: {}",
        String::from_utf8_lossy(&porcelain.stdout)
    );
}
