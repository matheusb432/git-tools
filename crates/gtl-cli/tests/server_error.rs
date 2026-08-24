use anyhow::{Context as _, Result};
use assert_cmd::Command;

mod common;

#[test]
fn server_request_failure_is_rendered_once_at_the_command_boundary() -> Result<()> {
    let empty_path = tempfile::tempdir().context("create empty executable path")?;
    // This integration-test target has one test, so PATH is fixed before its server thread starts.
    unsafe {
        std::env::set_var("PATH", empty_path.path());
    }
    let _server = common::ServerHarness::start(None, None)?;

    Command::new(env!("CARGO_BIN_EXE_git-tools"))
        .args(["status", "--all"])
        .assert()
        .code(2)
        .stdout("")
        .stderr("status: project catalogue is temporarily unavailable\n");
    Ok(())
}
