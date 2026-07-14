use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn passing_command_is_terse_and_writes_a_log() {
    let root = tempfile::tempdir().expect("temporary repository root");
    let mut command = Command::cargo_bin("gate").expect("gate binary");
    command
        .current_dir(root.path())
        .args(["quality", "printf 'hidden output\\n'"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(
                "RESULT scope=quality status=PASS log=.artifacts/logs/quality.log",
            )
            .and(predicate::str::contains("hidden output").not()),
        );

    assert_eq!(
        fs::read_to_string(root.path().join(".artifacts/logs/quality.log"))
            .expect("durable gate log"),
        "hidden output\n"
    );
}

#[test]
fn failing_command_reports_a_tail_and_preserves_the_exit_failure() {
    let root = tempfile::tempdir().expect("temporary repository root");
    let mut command = Command::cargo_bin("gate").expect("gate binary");
    command
        .current_dir(root.path())
        .args(["lint", "printf 'failure detail\\n'; exit 7"])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "RESULT scope=lint status=FAIL log=.artifacts/logs/lint.log",
        ))
        .stderr(predicate::str::contains("failure detail"));
}

#[test]
fn verbose_mode_replays_command_output() {
    let root = tempfile::tempdir().expect("temporary repository root");
    let mut command = Command::cargo_bin("gate").expect("gate binary");
    command
        .current_dir(root.path())
        .args(["--verbose", "tests", "printf 'visible output\\n'"])
        .assert()
        .success()
        .stderr(predicate::str::contains("visible output"));
}

#[test]
fn unsafe_scope_is_rejected_before_execution() {
    let root = tempfile::tempdir().expect("temporary repository root");
    let mut command = Command::cargo_bin("gate").expect("gate binary");
    command
        .current_dir(root.path())
        .args(["../escape", "exit 0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("filesystem-safe scope"));

    assert!(!root.path().join("../escape.log").exists());
}
