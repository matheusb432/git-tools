use anyhow::Result;
use assert_cmd::Command;

mod common;

#[test]
fn server_request_failure_is_rendered_once_at_the_command_boundary() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let config = directory.path().join("config.toml");
    std::fs::write(&config, "[diff.exclude]\ndefaults = [\"md\"]\n")?;
    let _server = common::ServerHarness::start(Some(&config), None)?;

    Command::new(env!("CARGO_BIN_EXE_git-tools"))
        .args(["push", "--all", "--dry"])
        .assert()
        .code(2)
        .stdout("")
        .stderr("user settings are invalid\n");
    Ok(())
}
