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
mod tests {
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{Duration, Instant},
    };

    use command_group::CommandGroup;

    use super::{terminate_and_reap, wait_for_exit};

    #[test]
    fn failed_parent_cleanup_terminates_its_remaining_process_group() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let child_pid_path = temporary_directory.path().join("child.pid");
        let mut command = Command::new("sh");
        command
            .env("GTL_E2E_CHILD_PID_PATH", &child_pid_path)
            .args([
                "-c",
                "sleep 2147483647 & printf '%s' \"$!\" > \"$GTL_E2E_CHILD_PID_PATH\"; exit 7",
            ]);
        let mut child = command.group_spawn().unwrap();
        let status = wait_for_exit(
            &mut child,
            Duration::from_secs(1),
            "failing process-group fixture",
        )
        .unwrap();
        let child_pid = fs::read_to_string(&child_pid_path)
            .unwrap()
            .parse::<u32>()
            .unwrap();

        assert!(!status.success());
        terminate_and_reap(&mut child, "failing process-group fixture").unwrap();
        assert!(wait_for_process_exit(child_pid, Duration::from_secs(1)));
    }

    fn wait_for_process_exit(pid: u32, duration: Duration) -> bool {
        let process_path = PathBuf::from(format!("/proc/{pid}"));
        let deadline = Instant::now() + duration;
        loop {
            if !process_path.exists() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
