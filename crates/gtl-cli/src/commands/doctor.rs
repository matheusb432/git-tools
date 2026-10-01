use gtl_local_transport::{
    LocalEndpoint,
    service::{ServerService, State},
};
use gtl_wire::doctor::{Check, CheckStatus, DoctorReport};

use super::server_ctl;
use crate::{ExitCode, cli::DoctorArgs};

pub(crate) fn run(args: &DoctorArgs) -> anyhow::Result<ExitCode> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(inspect());
    if args.json {
        println!("{}", serde_json::to_string(&report)?);
    } else {
        print!("{}", render_report(&report));
    }
    Ok(if report.failed() {
        ExitCode::Failed
    } else {
        ExitCode::Ok
    })
}

async fn inspect() -> DoctorReport {
    let mut checks = Vec::new();
    checks.push(match std::env::current_exe() {
        Ok(path) => Check::pass(
            "CLI executable",
            format!("{} (version {})", path.display(), env!("CARGO_PKG_VERSION")),
        ),
        Err(error) => Check::fail(
            "CLI executable",
            error.to_string(),
            "Check this installation's executable path.",
        ),
    });
    match diagnostic_command(&mut checks).await {
        Ok(mut command) => {
            checks.push(Check::pass(
                "server executable",
                command.as_std().get_program().to_string_lossy(),
            ));
            match server_ctl::inspect_server(&mut command).await {
                Ok(report) => {
                    if let Ok(endpoint) = LocalEndpoint::from_environment() && report.checks.iter().any(|check| check.name == "endpoint" && check.status == CheckStatus::Pass && check.detail != endpoint.path().display().to_string()) {
                        checks.push(Check::fail("endpoint configuration", "CLI and registered service use different endpoints", "Use the registered GIT_TOOLS_DATA_DIR, or run `gtl server install` with the intended configuration."));
                    }
                    checks.extend(report.checks);
                }
                Err(error) => checks.push(Check::fail("server diagnostics", format!("{error:#}"), "Inspect the registered executable and install matching binaries with `just cli update`.")),
            }
        }
        Err(error) => {
            checks.push(Check::fail("service configuration", format!("{error:#}"), "Inspect the native service registration; run `gtl server install` to register the intended sibling server."));
            if let Ok(server) = server_ctl::server_executable() {
                match server_ctl::inspect_server(&mut tokio::process::Command::new(server)).await {
                    Ok(report) => checks.extend(report.checks),
                    Err(error) => checks.push(Check::fail(
                        "server diagnostics",
                        format!("{error:#}"),
                        "Install matching binaries with `just cli update`.",
                    )),
                }
            }
        }
    }
    checks.push(match LocalEndpoint::from_environment() {
        Ok(endpoint) => Check::pass("client endpoint", endpoint.path().display().to_string()),
        Err(error) => Check::fail(
            "client endpoint",
            error.to_string(),
            "Correct GIT_TOOLS_DATA_DIR and the endpoint directory permissions.",
        ),
    });
    checks.push(match server_ctl::probe_rpc().await {
        Ok(info) if info.server_version == env!("CARGO_PKG_VERSION") => {
            Check::pass("RPC", format!("Serving, version {}", info.server_version))
        }
        Ok(info) => Check::fail(
            "RPC",
            format!(
                "Serving, server version {} differs from CLI {}",
                if info.server_version.is_empty() {
                    "unknown"
                } else {
                    &info.server_version
                },
                env!("CARGO_PKG_VERSION")
            ),
            "Install matching binaries, then run `gtl server restart`.",
        ),
        Err(error) => Check::fail(
            "RPC",
            format!("{error:#}"),
            "Resolve failed checks, then run `gtl server install` or `gtl server start`.",
        ),
    });
    DoctorReport {
        version: env!("CARGO_PKG_VERSION").into(),
        checks,
    }
}

async fn diagnostic_command(checks: &mut Vec<Check>) -> anyhow::Result<tokio::process::Command> {
    let service = ServerService::discover()?;
    let state = service.status().await?;
    checks.push(if state == State::Running {
        Check::pass("service", "Running")
    } else {
        Check::fail(
            "service",
            server_ctl::state_label(state),
            if state == State::NotInstalled {
                "Run `gtl server install`."
            } else {
                "Resolve failed checks, then run `gtl server start`."
            },
        )
    });
    checks.push(if service.startup_enabled().await? {
        Check::pass("login startup", "Enabled")
    } else {
        Check::fail(
            "login startup",
            "Disabled or not registered",
            "Run `gtl server install` to register login startup.",
        )
    });
    if state == State::NotInstalled {
        Ok(tokio::process::Command::new(
            server_ctl::server_executable()?
        ))
    } else {
        service.diagnostic_command().await
    }
}

pub(super) fn render_report(report: &DoctorReport) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    for check in &report.checks {
        let badge = match check.status {
            CheckStatus::Pass => "OK",
            CheckStatus::Warning => "WARN",
            CheckStatus::Fail => "FAIL",
        };
        let _ = writeln!(text, "{badge} :: {}", check.name);
        for line in check.detail.lines() {
            let _ = writeln!(text, "  {line}");
        }
        if let Some(action) = &check.action {
            let _ = writeln!(text, "  {action}");
        }
        text.push('\n');
    }
    text
}
