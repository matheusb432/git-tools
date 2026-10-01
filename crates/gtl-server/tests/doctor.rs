use std::{fs, path::PathBuf, process::Command};

use anyhow::{Context as _, Result};
use gtl_wire::doctor::{CheckStatus, DoctorReport};

#[test]
fn offline_diagnostics_report_failures_without_initializing_or_repairing_state() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let data = temporary.path().join("data");
    let config = temporary.path().join("config.toml");
    let server = std::env::var_os("GTL_CLI_TEST_BINARY").map_or_else(
        || PathBuf::from(env!("CARGO_BIN_EXE_gtl-server")),
        |cli| {
            PathBuf::from(cli).with_file_name(format!("gtl-server{}", std::env::consts::EXE_SUFFIX))
        },
    );
    let inspect = |hide_git: bool| -> Result<(bool, DoctorReport)> {
        let mut command = Command::new(&server);
        command
            .arg("doctor")
            .env("GIT_TOOLS_DATA_DIR", &data)
            .env("GIT_TOOLS_CONFIG", &config);
        if hide_git {
            command.env("PATH", temporary.path());
        }
        let output = command.output()?;
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok((
            output.status.success(),
            serde_json::from_slice(&output.stdout).context("server doctor JSON")?,
        ))
    };
    let (success, report) = inspect(false)?;
    assert!(success);
    assert!(!report.failed());
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "migrations" && check.status == CheckStatus::Warning)
    );
    assert!(!data.exists());
    assert!(!config.exists());

    fs::write(&config, "[push]\nconfirm = \"invalid\"\n")?;
    let (success, report) = inspect(false)?;
    assert!(!success);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "settings" && check.status == CheckStatus::Fail)
    );
    assert_eq!(
        fs::read_to_string(&config)?,
        "[push]\nconfirm = \"invalid\"\n"
    );
    assert!(!data.exists());

    fs::remove_file(&config)?;
    let (success, report) = inspect(true)?;
    assert!(!success);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "Git" && check.status == CheckStatus::Fail)
    );

    fs::create_dir(&data)?;
    let database = data.join("gtl.db");
    fs::write(&database, "corrupt database")?;
    let (success, report) = inspect(false)?;
    assert!(!success);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "database health" && check.status == CheckStatus::Fail)
    );
    assert_eq!(fs::read_to_string(&database)?, "corrupt database");
    Ok(())
}
