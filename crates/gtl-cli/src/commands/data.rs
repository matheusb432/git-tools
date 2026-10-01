use std::{
    ffi::OsStr,
    process::{Command, ExitStatus},
};

use anyhow::{Context as _, Result};

use crate::{
    ExitCode,
    cli::{DataExportArgs, DataImportArgs},
};

const SERVER_REFUSED_STATUS: i32 = 3;

pub fn export(args: &DataExportArgs) -> Result<ExitCode> {
    let status = run_server_data(&[
        OsStr::new("export"),
        OsStr::new("--to"),
        args.to.as_os_str(),
    ])?;
    Ok(exit_code_from(status))
}

pub fn import(args: &DataImportArgs) -> Result<ExitCode> {
    let from = [OsStr::new("--from"), args.from.as_os_str()];
    let staged = run_server_data(&[OsStr::new("stage-import"), from[0], from[1]])?;
    if !staged.success() {
        return Ok(exit_code_from(staged));
    }

    let server_was_active = server_lifecycle::is_active()?;
    if server_was_active {
        server_lifecycle::stop()?;
    }
    let finished = run_server_data(&[OsStr::new("finish-import"), from[0], from[1]])?;
    if !finished.success() {
        return Ok(exit_code_from(finished));
    }
    if server_was_active {
        server_lifecycle::start()
            .context("git-tools data was imported, but gtl-server did not start")?;
    }
    eprintln!("imported git-tools data from {}", args.from.display());
    Ok(ExitCode::Ok)
}

fn run_server_data(arguments: &[&OsStr]) -> Result<ExitStatus> {
    let server = super::server_ctl::server_executable()?;
    Command::new(&server)
        .arg("data")
        .args(arguments)
        .status()
        .with_context(|| format!("running {}", server.display()))
}

fn exit_code_from(status: ExitStatus) -> ExitCode {
    match status.code() {
        Some(0) => ExitCode::Ok,
        Some(SERVER_REFUSED_STATUS) => ExitCode::Refused,
        _ => ExitCode::Failed,
    }
}

mod server_lifecycle {
    use anyhow::Result;
    use gtl_local_transport::service::{ServerService, State};

    fn invoke<T>(action: impl AsyncFnOnce(ServerService) -> Result<T>) -> Result<T> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async { action(ServerService::discover()?).await })
    }

    pub fn is_active() -> Result<bool> {
        invoke(async |service| Ok(matches!(service.status().await?, State::Running)))
    }
    pub fn stop() -> Result<()> {
        invoke(async |service| service.stop().await)
    }
    pub fn start() -> Result<()> {
        invoke(async |service| service.start().await)
    }
}
