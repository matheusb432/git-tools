use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn server_request_failure_is_rendered_once_at_the_command_boundary() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("config.toml");
    std::fs::write(&config, "[push]\nconfirm = \"yes\"\n")?;
    let _server = common::ServerHarness::start(Some(&config), None)?;

    for arguments in [
        &["project", "push", "--all", "--dry"][..],
        &["diff", "HEAD", "--raw"][..],
    ] {
        Command::new(env!("CARGO_BIN_EXE_git-tools"))
            .args(arguments)
            .assert()
            .code(3)
            .stdout("")
            .stderr(
                predicate::str::contains(format!(
                    "User settings at {} are invalid",
                    config.display()
                ))
                .count(1)
                .and(predicate::str::contains(
                    "\n  `push.confirm` must be a boolean",
                )),
            );
    }
    Ok(())
}
