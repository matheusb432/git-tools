#![cfg(target_os = "linux")]

use std::fs;

use anyhow::Result;
use gtl_wire::doctor::{Check, CheckStatus, DoctorReport};
use predicates::prelude::*;

#[path = "support/server_fixture.rs"]
mod server_fixture;
use server_fixture::ServerFixture;

#[test]
fn server_and_doctor_help_parse_before_service_mutation() -> Result<()> {
    let fixture = ServerFixture::new()?;
    let help = fixture
        .command(&["server", "--help"])
        .assert()
        .success()
        .stderr("")
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(help)?;
    for verb in ["install", "uninstall", "start", "stop", "restart", "status"] {
        assert!(help.contains(verb));
    }
    fixture
        .command(&["doctor", "--help"])
        .assert()
        .success()
        .stderr("")
        .stdout(predicate::str::contains("--json"));
    fixture
        .command(&["server", "install", "--unknown"])
        .assert()
        .code(2)
        .stdout("");
    assert!(!fixture.config_home().exists());
    fixture
        .command(&["server", "start"])
        .assert()
        .failure()
        .stdout("");
    assert_eq!(fixture.service_state()?, None);
    Ok(())
}

#[test]
fn lifecycle_manages_registration_and_retains_binaries_and_data() -> Result<()> {
    let fixture = ServerFixture::new()?;
    let marker = fixture.data_root().join("retained-data");
    fs::write(&marker, "retained")?;
    fixture
        .command(&["server", "install"])
        .assert()
        .success()
        .stderr("")
        .stdout(predicate::str::contains("installed and ready"));
    let unit = fs::read_to_string(fixture.unit_path())?;
    assert!(unit.contains(&format!(
        "ExecStart=\"{}\"",
        fixture.server_path().display()
    )));
    assert!(unit.contains("UMask=0077"));
    fixture
        .command(&["server", "status"])
        .assert()
        .success()
        .stderr("")
        .stdout(
            predicate::str::contains("service: running")
                .and(predicate::str::contains("startup: enabled"))
                .and(predicate::str::contains("health: serving")),
        );
    fixture
        .command(&["server", "restart"])
        .assert()
        .success()
        .stderr("");
    fixture
        .command(&["server", "stop"])
        .assert()
        .success()
        .stderr("");
    assert_eq!(fixture.service_state()?.as_deref(), Some("stopped"));
    assert!(fixture.unit_path().exists());
    fixture
        .command(&["server", "start"])
        .assert()
        .success()
        .stderr("");
    fixture
        .command(&["server", "uninstall"])
        .assert()
        .success()
        .stderr("");
    assert!(!fixture.unit_path().exists());
    assert!(fixture.server_path().exists());
    assert!(fixture.data_root().join("gtl.db").is_file());
    assert_eq!(fs::read_to_string(&marker)?, "retained");
    fixture
        .command(&["server", "uninstall"])
        .assert()
        .success()
        .stderr("");
    Ok(())
}

#[test]
fn doctor_reports_registered_configuration_and_rpc_health() -> Result<()> {
    let fixture = ServerFixture::installed()?;
    let report = fixture.doctor_report(0)?;
    assert!(!report.failed());
    assert!(report.checks.iter().any(|check| check.name == "RPC"
        && check.status == CheckStatus::Pass
        && check.detail.contains("Serving")));
    fixture
        .command(&["doctor", "--json"])
        .env("GTL_TEST_ENABLED", "0")
        .assert()
        .code(1)
        .stderr("")
        .stdout(predicate::str::contains("login startup"));
    fixture.set_report(&DoctorReport {
        version: env!("CARGO_PKG_VERSION").into(),
        checks: vec![Check::pass("endpoint", "/different/endpoint")],
    })?;
    let report = fixture.doctor_report(1)?;
    assert!(
        report.checks.iter().any(
            |check| check.name == "endpoint configuration" && check.status == CheckStatus::Fail
        )
    );
    fixture
        .command(&["doctor", "--json"])
        .env(
            "GIT_TOOLS_DATA_DIR",
            fixture.data_root().join("unavailable"),
        )
        .assert()
        .code(1)
        .stderr("")
        .stdout(predicate::str::contains("RPC"));
    Ok(())
}

#[test]
fn restart_preflight_preserves_running_service_on_incompatible_configuration() -> Result<()> {
    let fixture = ServerFixture::installed()?;
    let unit_before = fs::read(fixture.unit_path())?;
    let version = env!("CARGO_PKG_VERSION");
    for (report, diagnostic) in [
        (
            DoctorReport {
                version: "incompatible".into(),
                checks: vec![],
            },
            "differs from CLI",
        ),
        (
            DoctorReport {
                version: version.into(),
                checks: vec![Check::pass("endpoint", "/different/endpoint")],
            },
            "different endpoints",
        ),
        (
            DoctorReport {
                version: version.into(),
                checks: vec![Check::fail(
                    "settings",
                    "invalid setting",
                    "Correct settings.",
                )],
            },
            "invalid setting",
        ),
    ] {
        fixture.set_report(&report)?;
        fixture
            .command(&["server", "restart"])
            .env(
                "GTL_TEST_REPORT_EXIT",
                if report.failed() { "1" } else { "0" },
            )
            .assert()
            .code(1)
            .stdout("")
            .stderr(predicate::str::contains(diagnostic));
        assert_eq!(fixture.service_state()?.as_deref(), Some("running"));
        assert_eq!(fs::read(fixture.unit_path())?, unit_before);
    }
    Ok(())
}

#[test]
fn doctor_renders_failed_checks_on_stdout_with_failure_status() -> Result<()> {
    let fixture = ServerFixture::installed()?;
    fixture.set_report(&DoctorReport {
        version: env!("CARGO_PKG_VERSION").into(),
        checks: vec![Check::fail(
            "settings",
            "invalid setting",
            "Correct settings.",
        )],
    })?;
    fixture
        .command(&["doctor"])
        .env("GTL_TEST_REPORT_EXIT", "1")
        .assert()
        .code(1)
        .stderr("")
        .stdout(
            predicate::str::contains("FAIL :: settings")
                .and(predicate::str::contains("Correct settings.")),
        );
    Ok(())
}
