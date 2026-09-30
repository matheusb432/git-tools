use std::{
    ffi::OsStr,
    path::PathBuf,
    process::{Command, ExitStatus},
};

use anyhow::{Context as _, Result, ensure};

use crate::{
    ExitCode,
    cli::{DataExportArgs, DataImportArgs},
};

const SERVER_EXECUTABLE_NAME: &str = "gtl-server";
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

    #[cfg(target_os = "linux")]
    let server_was_active = server_lifecycle::is_active()?;
    #[cfg(target_os = "linux")]
    if server_was_active {
        server_lifecycle::stop()?;
    }
    let finished = run_server_data(&[OsStr::new("finish-import"), from[0], from[1]])?;
    if !finished.success() {
        return Ok(exit_code_from(finished));
    }
    #[cfg(target_os = "linux")]
    if server_was_active {
        server_lifecycle::start()
            .context("git-tools data was imported, but gtl-server did not start")?;
    }
    eprintln!("imported git-tools data from {}", args.from.display());
    Ok(ExitCode::Ok)
}

fn run_server_data(arguments: &[&OsStr]) -> Result<ExitStatus> {
    let server = server_executable()?;
    Command::new(&server)
        .arg("data")
        .args(arguments)
        .status()
        .with_context(|| format!("running {}", server.display()))
}

fn server_executable() -> Result<PathBuf> {
    let cli = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .context("resolving the git-tools executable")?;
    let server = cli.with_file_name(format!(
        "{SERVER_EXECUTABLE_NAME}{}",
        std::env::consts::EXE_SUFFIX
    ));
    ensure!(
        server.is_file(),
        "missing sibling {}; install gtl-server beside git-tools with `just update`",
        server.display()
    );
    Ok(server)
}

fn exit_code_from(status: ExitStatus) -> ExitCode {
    match status.code() {
        Some(0) => ExitCode::Ok,
        Some(SERVER_REFUSED_STATUS) => ExitCode::Refused,
        _ => ExitCode::Failed,
    }
}

#[cfg(target_os = "linux")]
mod server_lifecycle {
    use std::process::Command;

    use anyhow::{Context as _, Result, ensure};

    const SERVER_UNIT_NAME: &str = "gtl-server.service";

    pub fn is_active() -> Result<bool> {
        let status = Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", SERVER_UNIT_NAME])
            .status()
            .context("running systemctl to inspect gtl-server")?;
        Ok(status.success())
    }

    pub fn stop() -> Result<()> {
        systemctl_user("stop")
    }

    pub fn start() -> Result<()> {
        systemctl_user("start")
    }

    fn systemctl_user(verb: &str) -> Result<()> {
        let status = Command::new("systemctl")
            .args(["--user", verb, SERVER_UNIT_NAME])
            .status()
            .with_context(|| format!("running systemctl --user {verb} {SERVER_UNIT_NAME}"))?;
        ensure!(
            status.success(),
            "systemctl --user {verb} {SERVER_UNIT_NAME} failed with {status}"
        );
        Ok(())
    }
}
