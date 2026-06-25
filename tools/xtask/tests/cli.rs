//! Arg-surface tests: build the binary and assert the verb surface clap derives. Add a case
//! per real verb, and assert the *message* on conflicts (`contains("cannot be used with")`),
//! not a bare exit-2 (exit 2 also fires on an unknown subcommand). See rust-tests /
//! rust-cli-tooling.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_the_verb_surface() {
    Command::cargo_bin("xtask")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("bootstrap"))
        .stdout(predicates::str::contains("check"));
}

#[test]
fn bootstrap_is_a_known_verb() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["bootstrap", "--help"])
        .assert()
        .success();
}

#[test]
fn check_exposes_its_flags() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["check", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--verbose"));
}

#[test]
fn unknown_verb_is_rejected() {
    Command::cargo_bin("xtask")
        .unwrap()
        .arg("definitely-not-a-verb")
        .assert()
        .failure()
        .stderr(
            predicates::str::contains("unrecognized subcommand")
                .or(predicates::str::contains("unexpected argument")),
        );
}
