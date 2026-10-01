use std::{path::PathBuf, time::Duration};

use anyhow::{Context as _, ensure};
use gtl_client::GtlClient;
use gtl_local_transport::{
    LocalEndpoint,
    service::{ServerService, State, output},
};
use gtl_wire::doctor::DoctorReport;
use tokio::{
    process::Command,
    time::{sleep, timeout},
};

use crate::cli::ServerCommand;

pub(crate) fn run(command: &ServerCommand) -> anyhow::Result<String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run_async(command))
}

async fn run_async(command: &ServerCommand) -> anyhow::Result<String> {
    let service = ServerService::discover()?;
    match command {
        ServerCommand::Install => {
            let server = server_executable()?;
            preflight(&mut Command::new(&server)).await?;
            service.stop().await?;
            service.install(&server).await?;
            service.start().await?;
            wait_ready(&service).await?;
            Ok(format!(
                "gtl-server installed and ready\nexecutable: {}",
                server.display()
            ))
        }
        ServerCommand::Uninstall => {
            service.uninstall().await?;
            Ok("gtl-server startup registration removed".into())
        }
        ServerCommand::Start | ServerCommand::Restart => {
            preflight(&mut service.diagnostic_command().await?).await?;
            if matches!(command, ServerCommand::Restart) {
                service.stop().await?;
            }
            service.start().await?;
            wait_ready(&service).await?;
            Ok("gtl-server ready".into())
        }
        ServerCommand::Stop => {
            service.stop().await?;
            Ok("gtl-server stopped".into())
        }
        ServerCommand::Status => {
            let state = state_label(service.status().await?);
            let startup = if service.startup_enabled().await? {
                "enabled"
            } else {
                "disabled"
            };
            let (health, version, endpoint) = match probe_rpc().await {
                Ok(info) => (
                    "serving",
                    if info.server_version.is_empty() {
                        "unknown".into()
                    } else {
                        info.server_version
                    },
                    format!("\nendpoint: {}", info.endpoint.display()),
                ),
                Err(error) => (
                    "unavailable",
                    "unknown".into(),
                    format!("\nhealth detail: {error:#}\nRun `gtl doctor` for recovery actions."),
                ),
            };
            Ok(format!(
                "service: {state}\nstartup: {startup}\nhealth: {health}\nclient version: {}\nserver version: {version}{endpoint}",
                env!("CARGO_PKG_VERSION")
            ))
        }
    }
}

pub(super) fn server_executable() -> anyhow::Result<PathBuf> {
    let cli = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .context("resolving the git-tools executable")?;
    let server = cli.with_file_name(format!("gtl-server{}", std::env::consts::EXE_SUFFIX));
    ensure!(
        server.is_file(),
        "missing sibling {}; install gtl-server beside git-tools with `just cli update`",
        server.display()
    );
    Ok(server)
}

pub(super) fn state_label(state: State) -> &'static str {
    match state {
        State::NotInstalled => "not installed",
        State::Stopped => "stopped",
        State::Running => "running",
        State::Failed => "failed",
        #[cfg(target_os = "linux")]
        State::Starting => "starting",
    }
}

pub(super) async fn inspect_server(command: &mut Command) -> anyhow::Result<DoctorReport> {
    let response = output(command.arg("doctor")).await?;
    let report: DoctorReport = serde_json::from_slice(&response.stdout).with_context(|| format!("server could not return diagnostics ({}): {}; install matching binaries with `just cli update`", response.status, String::from_utf8_lossy(&response.stderr).trim()))?;
    ensure!(
        report.version == env!("CARGO_PKG_VERSION"),
        "server version {} differs from CLI {}; install matching binaries",
        report.version,
        env!("CARGO_PKG_VERSION")
    );
    ensure!(
        response.status.success() || report.failed(),
        "server exited with {} despite a passing diagnostic report",
        response.status
    );
    Ok(report)
}

async fn preflight(command: &mut Command) -> anyhow::Result<()> {
    let report = inspect_server(command).await?;
    ensure!(
        !report.failed(),
        "gtl-server cannot start:\n{}",
        super::doctor::render_report(&report)
    );
    let endpoint = LocalEndpoint::from_environment()?;
    ensure!(
        !report.checks.iter().any(|check| check.name == "endpoint"
            && check.detail != endpoint.path().display().to_string()),
        "CLI and registered service use different endpoints; use the registered GIT_TOOLS_DATA_DIR or run `gtl server install` with the intended configuration"
    );
    Ok(())
}

pub(super) struct RpcInfo {
    pub(super) server_version: String,
    pub(super) endpoint: PathBuf,
}

pub(super) async fn probe_rpc() -> anyhow::Result<RpcInfo> {
    timeout(Duration::from_secs(6), async {
        let client = GtlClient::connect_local().await?;
        let info = client.get_viewer_server_info().await?;
        Ok::<_, anyhow::Error>(RpcInfo {
            server_version: info.server_version().into(),
            endpoint: client.endpoint().path().to_path_buf(),
        })
    })
    .await
    .context("gtl-server RPC probe timed out after 6 seconds")?
}

async fn wait_ready(service: &ServerService) -> anyhow::Result<()> {
    timeout(Duration::from_secs(15), async {
        loop {
            let state = service.status().await?;
            if state == State::Failed {
                anyhow::bail!("gtl-server failed during startup: {}\nRun `gtl doctor` for recovery actions.", service.failure_detail().await?.trim());
            }
            if state == State::Running && let Ok(info) = probe_rpc().await {
                ensure!(info.server_version == env!("CARGO_PKG_VERSION"), "running server version {} differs from CLI {}; run `gtl server install` with matching binaries", info.server_version, env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
    }).await.context("gtl-server did not become ready within 15 seconds; run `gtl doctor` for recovery actions")?
}
