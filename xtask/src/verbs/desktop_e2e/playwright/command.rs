use std::{
    fs::{self, File},
    io::Write as _,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow, bail};
use command_group::{CommandGroup, GroupChild};

use super::super::{HostCargoEnvironment, IsolatedEnv, Sandbox};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const PROCESS_REAP_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn run(
    sandbox: &Sandbox,
    environment: &IsolatedEnv,
    host_environment: &HostCargoEnvironment,
    log_name: &str,
    arguments: &[&str],
    description: &str,
    timeout: Duration,
) -> Result<()> {
    let log = sandbox.logs.join(log_name);
    let stdout =
        File::create(&log).with_context(|| format!("create Playwright log {}", log.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("clone Playwright log {}", log.display()))?;
    let mut command = Command::new("cargo");
    command
        .arg("--config")
        .arg(&sandbox.cargo_runner_config)
        .args(arguments)
        .current_dir(".")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    environment.apply_cargo(&mut command, host_environment);
    let mut child = command
        .group_spawn()
        .with_context(|| format!("{description}: start cargo"))?;
    let status = wait_for_exit(&mut child, timeout, description);
    let bytes = fs::read(&log).with_context(|| format!("read Playwright log {}", log.display()))?;
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            std::io::stderr().write_all(&bytes)?;
            return match terminate_and_reap(&mut child, description) {
                Ok(()) => Err(error),
                Err(cleanup_error) => {
                    Err(error.context(format!("process cleanup also failed: {cleanup_error:#}")))
                }
            };
        }
    };
    if status.success() {
        return Ok(());
    }
    std::io::stderr().write_all(&bytes)?;
    let error = anyhow!(
        "{description} failed (exit {})",
        status.code().unwrap_or(-1)
    );
    match terminate_and_reap(&mut child, description) {
        Ok(()) => Err(error),
        Err(cleanup_error) => {
            Err(error.context(format!("process cleanup also failed: {cleanup_error:#}")))
        }
    }
}

fn wait_for_exit(
    child: &mut GroupChild,
    timeout: Duration,
    description: &str,
) -> Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("poll {description} process group"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            bail!(
                "{description} timed out after {} seconds",
                timeout.as_secs()
            );
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

fn terminate_and_reap(child: &mut GroupChild, description: &str) -> Result<()> {
    if let Err(error) = child.kill()
        && child
            .try_wait()
            .with_context(|| format!("check {description} after termination error"))?
            .is_none()
    {
        return Err(error).context(format!("terminate {description} process group"));
    }
    let deadline = Instant::now() + PROCESS_REAP_TIMEOUT;
    loop {
        if child
            .try_wait()
            .with_context(|| format!("reap {description} process group"))?
            .is_some()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "{description} process group did not reap within {} seconds",
                PROCESS_REAP_TIMEOUT.as_secs()
            );
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../../../../e2e/gtl-desktop-e2e/xtask/command.rs"]
mod tests;
