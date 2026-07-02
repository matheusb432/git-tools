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
        .stdout(predicates::str::contains("install"))
        .stdout(predicates::str::contains("uninstall"))
        .stdout(predicates::str::contains("gen-icon"))
        .stdout(predicates::str::contains("ship"));
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
fn gen_icon_is_a_known_verb() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["gen-icon", "--help"])
        .assert()
        .success();
}

#[test]
fn install_exposes_its_target_flag() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["install", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--target"));
}

#[test]
fn uninstall_exposes_its_flags() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["uninstall", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--remove-config"))
        .stdout(predicates::str::contains("--force"));
}

#[test]
fn test_exposes_its_flags() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["test", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--verbose"))
        .stdout(predicates::str::contains("--all"));
}

#[test]
fn drift_check_is_a_known_verb() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["drift-check", "--help"])
        .assert()
        .success();
}

#[test]
fn check_deps_is_a_known_verb() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["check-deps", "--help"])
        .assert()
        .success();
}

#[test]
fn check_structure_is_a_known_verb() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["check-structure", "--help"])
        .assert()
        .success();
}

#[test]
fn fmt_exposes_its_check_flag() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["fmt", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--check"));
}

#[test]
fn ship_exposes_its_smoke_flag() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["ship", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--smoke"));
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
