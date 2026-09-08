use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn server_request_failure_is_rendered_once_at_the_command_boundary() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("config.toml");
    std::fs::write(&config, "[diff.exclude]\ndefaults = [\"md\"]\n")?;
    let _server = common::ServerHarness::start(Some(&config), None)?;

    for (arguments, exit_code) in [
        (["push", "--all", "--dry"], 2),
        (["diff", "HEAD", "--raw"], 1),
    ] {
        Command::new(env!("CARGO_BIN_EXE_git-tools"))
            .args(arguments)
            .assert()
            .code(exit_code)
            .stdout("")
            .stderr(
                predicate::str::contains(format!(
                    "user settings at {} are invalid",
                    config.display()
                ))
                .count(1)
                .and(predicate::str::contains(
                    "`diff.exclude` must be an array of strings",
                )),
            );
    }
    Ok(())
}
