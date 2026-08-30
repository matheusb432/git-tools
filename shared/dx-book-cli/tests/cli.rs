use assert_cmd::Command;
use predicates::prelude::*;

fn dx_book() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dx-book"))
}

#[test]
fn help_lists_the_book_commands() {
    dx_book()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("styles"));
}

#[test]
fn serve_help_lists_holt_shaped_overrides() {
    dx_book()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--port"))
        .stdout(predicate::str::contains("--open"));
}

#[test]
fn serve_rejects_an_invalid_project_configuration() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("dx-book.toml"), "invalid = [[[").unwrap();

    dx_book()
        .current_dir(project.path())
        .arg("serve")
        .assert()
        .failure()
        .stderr(predicate::str::contains("load dx-book.toml"));
}

#[test]
fn serve_accepts_additional_dioxus_arguments() {
    let project = tempfile::tempdir().unwrap();

    dx_book()
        .current_dir(project.path())
        .args(["serve", "--addr", "127.0.0.1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("load dx-book.toml"))
        .stderr(predicate::str::contains("unexpected argument").not());
}
