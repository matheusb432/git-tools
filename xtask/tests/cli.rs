//! Arg-surface tests: build the binary and assert the verb surface clap derives. Add a case
//! per real verb, and assert the *message* on conflicts (`contains("cannot be used with")`),
//! not a bare exit-2 (exit 2 also fires on an unknown subcommand). See the `cli-best-practices`
//! skill.

use assert_cmd::Command;

#[test]
fn forced_color_help_uses_cargo_palette() {
    Command::cargo_bin("xtask")
        .unwrap()
        .arg("--help")
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .assert()
        .success()
        .stdout(predicates::str::contains("\u{1b}["))
        .stdout(predicates::str::contains("36m"));
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
fn pre_commit_is_a_known_verb() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["pre-commit", "--help"])
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
        .stdout(predicates::str::contains("--scope"))
        .stdout(predicates::str::contains("--e2e"))
        .stdout(predicates::str::contains("--all"));
}

#[test]
fn test_rejects_conflicting_expensive_scopes() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["test", "--e2e", "--all"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"));
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
fn quality_verbs_are_separate_subcommands() {
    Command::cargo_bin("xtask")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("fmt "))
        .stdout(predicates::str::contains("fmt-check"))
        .stdout(predicates::str::contains("lint"))
        .stdout(predicates::str::contains("check"));
}

#[test]
fn ship_exposes_smoke_and_force_flags() {
    Command::cargo_bin("xtask")
        .unwrap()
        .args(["ship", "--help"])
        .assert()
        .success()
        .stdout(predicates::str::contains("--smoke"))
        .stdout(predicates::str::contains("--force"));
}
