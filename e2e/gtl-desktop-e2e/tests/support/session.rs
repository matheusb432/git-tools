use std::{
    env, fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, ensure};
use command_group::{CommandGroup, GroupChild};
#[cfg(unix)]
use command_group::{Signal, UnixChildExt};
use serde_json::json;
use thirtyfour::{Capabilities, WebDriver};
use tokio::time::{Instant, sleep, timeout};

use super::wait::{self, WEBDRIVER_OPERATION_TIMEOUT};

const START_ATTEMPTS_MAX: usize = 5;
const START_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECTION_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const CHILD_EXIT_GRACE_TIMEOUT: Duration = Duration::from_millis(500);
const CHILD_TERMINATE_TIMEOUT: Duration = Duration::from_millis(500);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct TestSession {
    driver: Option<WebDriver>,
    driver_child: Option<GroupChild>,
    data_root: PathBuf,
}

impl TestSession {
    pub async fn start(name: &str) -> Result<Self> {
        let data_root = suite_data_root(name)?;
        Self::start_with_data_root(data_root).await
    }

    async fn start_with_data_root(data_root: PathBuf) -> Result<Self> {
        verify_runtime_environment()?;
        let viewer_binary = viewer_binary()?;
        let mut failure = None;

        for attempt in 1..=START_ATTEMPTS_MAX {
            let ports = available_port_pair().context("select distinct driver ports")?;
            let (webdriver_port, native_driver_port) = ports.ports()?;
            let mut command = Command::new("tauri-driver");
            command
                .arg("--port")
                .arg(webdriver_port.to_string())
                .arg("--native-port")
                .arg(native_driver_port.to_string())
                .env("GIT_TOOLS_DATA_DIR", &data_root);
            deny_external_proxies(&mut command);
            drop(ports);
            let mut driver_child = match command.group_spawn() {
                Ok(child) => child,
                Err(error) => {
                    failure = Some(
                        anyhow::Error::new(error)
                            .context(format!("spawn tauri-driver attempt {attempt}")),
                    );
                    continue;
                }
            };

            match timeout(
                START_ATTEMPT_TIMEOUT,
                connect_driver(webdriver_port, &viewer_binary, &mut driver_child),
            )
            .await
            {
                Ok(Ok(driver)) => {
                    return Ok(Self {
                        driver: Some(driver),
                        driver_child: Some(driver_child),
                        data_root,
                    });
                }
                Ok(Err(error)) => {
                    failure = Some(
                        error.context(format!("connect to tauri-driver on attempt {attempt}")),
                    );
                }
                Err(_) => {
                    failure = Some(anyhow!(
                        "tauri-driver attempt {attempt} exceeded {START_ATTEMPT_TIMEOUT:?}"
                    ));
                }
            }

            terminate_child(&mut driver_child)
                .await
                .with_context(|| format!("clean up tauri-driver attempt {attempt}"))?;
        }

        Err(failure.unwrap_or_else(|| anyhow!("tauri-driver did not start")))
    }

    pub fn driver(&self) -> &WebDriver {
        self.driver
            .as_ref()
            .expect("driver remains available until session cleanup")
    }

    pub fn driver_if_active(&self) -> Option<&WebDriver> {
        self.driver.as_ref()
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub async fn restart(&mut self) -> Result<()> {
        let data_root = self.data_root.clone();
        self.shutdown()
            .await
            .context("stop viewer before restart")?;
        let replacement = Self::start_with_data_root(data_root)
            .await
            .context("start viewer after restart")?;
        *self = replacement;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<()> {
        self.shutdown().await
    }

    async fn shutdown(&mut self) -> Result<()> {
        let driver_result = match self.driver.take() {
            Some(driver) => {
                wait::within(
                    "quit WebDriver session",
                    WEBDRIVER_OPERATION_TIMEOUT,
                    async { driver.quit().await.map_err(anyhow::Error::from) },
                )
                .await
            }
            None => Ok(()),
        };
        let child_result = match self.driver_child.as_mut() {
            Some(child) => terminate_child(child).await,
            None => Ok(()),
        };
        if child_result.is_ok() {
            self.driver_child.take();
        }

        match (driver_result, child_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(driver_error), Ok(())) => Err(driver_error),
            (Ok(()), Err(child_error)) => Err(child_error),
            (Err(driver_error), Err(child_error)) => Err(driver_error)
                .context(format!("tauri-driver cleanup also failed: {child_error:#}")),
        }
    }
}

fn suite_data_root(name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "desktop E2E suite name must contain only lowercase ASCII letters, digits, and hyphens"
    );
    let base = env::var_os("GTL_E2E_DATA_ROOT")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("GTL_E2E_DATA_ROOT is required"))?;
    let data_root = base.join(name);
    fs::create_dir_all(&data_root)
        .with_context(|| format!("create suite data root {}", data_root.display()))?;
    Ok(data_root)
}

impl Drop for TestSession {
    fn drop(&mut self) {
        if let Some(child) = self.driver_child.as_mut() {
            let _ = child.kill();
        }
    }
}

async fn connect_driver(
    port: u16,
    viewer_binary: &str,
    driver_child: &mut GroupChild,
) -> Result<WebDriver> {
    let mut capabilities = Capabilities::new();
    capabilities.set("browserName", "wry")?;
    capabilities.set("tauri:options", json!({ "application": viewer_binary }))?;
    let driver_url = format!("http://127.0.0.1:{port}");
    let deadline = Instant::now() + START_ATTEMPT_TIMEOUT;

    loop {
        match WebDriver::new(&driver_url, capabilities.clone()).await {
            Ok(driver) => return Ok(driver),
            Err(error) => {
                if driver_child
                    .try_wait()
                    .context("inspect tauri-driver launch")?
                    .is_some()
                {
                    return Err(anyhow::Error::new(error)
                        .context("tauri-driver exited before WebDriver session creation"));
                }
                if Instant::now() >= deadline {
                    return Err(error.into());
                }
                sleep(CONNECTION_RETRY_INTERVAL).await;
            }
        }
    }
}

fn verify_runtime_environment() -> Result<()> {
    ensure!(
        env::var("GTL_E2E_RUNTIME_ISOLATED").as_deref() == Ok("1"),
        "desktop E2E must run through the typed xtask runtime boundary"
    );
    for name in ["CARGO_HOME", "RUSTUP_HOME", "SSH_AUTH_SOCK"] {
        ensure!(
            env::var_os(name).is_none(),
            "desktop E2E runtime inherited host-only {name}"
        );
    }
    Ok(())
}

fn viewer_binary() -> Result<String> {
    let path = env::var_os("GTL_E2E_VIEWER_BINARY")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("GTL_E2E_VIEWER_BINARY is required"))?;
    path.canonicalize()
        .with_context(|| format!("canonicalize viewer binary {}", path.display()))
        .map(|path| path.to_string_lossy().into_owned())
}

fn deny_external_proxies(command: &mut Command) {
    for name in ["HTTP_PROXY", "http_proxy", "HTTPS_PROXY", "https_proxy"] {
        command.env(name, "http://127.0.0.1:9");
    }
    for name in ["NO_PROXY", "no_proxy"] {
        command.env(name, "127.0.0.1,localhost");
    }
}

struct DriverPorts {
    webdriver_listener: TcpListener,
    native_driver_listener: TcpListener,
}

impl DriverPorts {
    fn ports(&self) -> Result<(u16, u16)> {
        let webdriver_port = self
            .webdriver_listener
            .local_addr()
            .context("read WebDriver candidate port")?
            .port();
        let native_driver_port = self
            .native_driver_listener
            .local_addr()
            .context("read native driver candidate port")?
            .port();
        debug_assert_ne!(webdriver_port, native_driver_port);
        Ok((webdriver_port, native_driver_port))
    }
}

fn available_port_pair() -> Result<DriverPorts> {
    let webdriver_listener =
        TcpListener::bind(("127.0.0.1", 0)).context("bind WebDriver candidate port")?;
    let native_driver_listener =
        TcpListener::bind(("127.0.0.1", 0)).context("bind native driver candidate port")?;
    Ok(DriverPorts {
        webdriver_listener,
        native_driver_listener,
    })
}

async fn terminate_child(child: &mut GroupChild) -> Result<()> {
    if wait_for_exit(child, CHILD_EXIT_GRACE_TIMEOUT).await? {
        return Ok(());
    }

    if request_driver_group_termination(child).context("terminate tauri-driver process group")?
        && wait_for_exit(child, CHILD_TERMINATE_TIMEOUT).await?
    {
        return Ok(());
    }

    force_kill_driver_group(child).context("kill tauri-driver process group")?;
    if wait_for_exit(child, CLEANUP_TIMEOUT).await? {
        return Ok(());
    }

    Err(anyhow!(
        "tauri-driver process group did not exit within {CLEANUP_TIMEOUT:?} after kill"
    ))
}

#[cfg(unix)]
fn request_driver_group_termination(child: &GroupChild) -> std::io::Result<bool> {
    match child.signal(Signal::SIGTERM) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn request_driver_group_termination(_: &GroupChild) -> std::io::Result<bool> {
    Ok(false)
}

fn force_kill_driver_group(child: &mut GroupChild) -> std::io::Result<()> {
    match child.kill() {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
        Err(error) => Err(error),
    }
}

async fn wait_for_exit(child: &mut GroupChild, duration: Duration) -> Result<bool> {
    let deadline = Instant::now() + duration;
    loop {
        if child
            .try_wait()
            .context("inspect tauri-driver process group")?
            .is_some()
        {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        sleep(CLEANUP_POLL_INTERVAL.min(deadline - Instant::now())).await;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::TcpListener,
        path::{Path, PathBuf},
        process::Command,
        time::Duration,
    };

    use command_group::CommandGroup;

    use super::{available_port_pair, request_driver_group_termination, wait_for_exit};

    #[test]
    fn distinct_driver_ports_are_selected() {
        let ports = available_port_pair().unwrap();
        let (webdriver, native) = ports.ports().unwrap();
        assert_ne!(webdriver, native);
    }

    #[test]
    fn driver_ports_remain_reserved_while_the_pair_is_owned() {
        let ports = available_port_pair().unwrap();
        let (webdriver, native) = ports.ports().unwrap();

        assert!(TcpListener::bind(("127.0.0.1", webdriver)).is_err());
        assert!(TcpListener::bind(("127.0.0.1", native)).is_err());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn terminating_driver_reaps_its_process_group() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let child_pid_path = temporary_directory.path().join("child.pid");
        let mut command = Command::new("sh");
        command
            .env("GTL_E2E_CHILD_PID_PATH", &child_pid_path)
            .args([
                "-c",
                "sleep 2147483647 & child=$!; printf '%s' \"$child\" > \"$GTL_E2E_CHILD_PID_PATH\"; trap 'exit 0' TERM; wait",
            ]);
        let mut child = command.group_spawn().unwrap();
        let Some(child_pid) = wait_for_child_pid(&child_pid_path).await else {
            let _ = child.kill();
            let _ = wait_for_exit(&mut child, Duration::from_secs(1)).await;
            panic!("child pid was not written");
        };

        let termination_requested = request_driver_group_termination(&child);
        if !matches!(&termination_requested, Ok(true)) {
            let _ = child.kill();
            let _ = wait_for_exit(&mut child, Duration::from_secs(1)).await;
        }
        assert!(termination_requested.unwrap());
        let exited = wait_for_process_exit(child_pid, Duration::from_secs(1)).await;
        if !exited {
            let _ = child.kill();
        }
        let reaped = wait_for_exit(&mut child, Duration::from_secs(1)).await;

        assert!(exited);
        assert!(reaped.unwrap());
    }

    #[cfg(target_os = "linux")]
    async fn wait_for_child_pid(path: &Path) -> Option<u32> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        loop {
            if let Ok(pid) = fs::read_to_string(path)
                && let Ok(pid) = pid.trim().parse()
            {
                return Some(pid);
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[cfg(target_os = "linux")]
    async fn wait_for_process_exit(pid: u32, duration: Duration) -> bool {
        let process_path = PathBuf::from(format!("/proc/{pid}"));
        let deadline = tokio::time::Instant::now() + duration;
        loop {
            if !process_path.exists() {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
