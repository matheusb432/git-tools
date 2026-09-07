use std::{
    env, fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use command_group::{CommandGroup, GroupChild};
#[cfg(unix)]
use command_group::{Signal, UnixChildExt};
use serde_json::json;
use thirtyfour::{By, Capabilities, WebDriver, prelude::ElementQueryable as _};
use tokio::time::{Instant, sleep, timeout};

use super::wait::{self, WEBDRIVER_OPERATION_TIMEOUT};

const START_ATTEMPTS_MAX: usize = 5;
const START_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECTION_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const CHILD_EXIT_GRACE_TIMEOUT: Duration = Duration::from_millis(500);
const CHILD_TERMINATE_TIMEOUT: Duration = Duration::from_millis(500);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub struct TestSession {
    pub catalogue: super::catalogue::ProjectCatalogue,
    driver: Option<WebDriver>,
    driver_child: Option<GroupChild>,
    server_child: Option<GroupChild>,
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
        let catalogue = super::catalogue::ProjectCatalogue::open(&data_root)?;
        let mut server_child = start_server(&data_root).await?;
        match start_driver_with_retries(&data_root, &viewer_binary).await {
            Ok((driver, driver_child)) => Ok(Self {
                catalogue,
                driver: Some(driver),
                driver_child: Some(driver_child),
                server_child: Some(server_child),
                data_root,
            }),
            Err(error) => Err(cleanup_start_failure("gtl-server", &mut server_child, error).await),
        }
    }

    pub fn project_requests(&self) -> Result<usize> {
        let log = fs::read_to_string(self.data_root.join("server.stderr.log"))?;
        Ok(log
            .lines()
            .filter(|line| {
                line.contains("/gtl.v1.ViewerService/ListViewerProjects")
                    && line.contains("finished processing request")
            })
            .count())
    }

    #[cfg(unix)]
    pub fn pause_server(&self) -> Result<ServerPause<'_>> {
        let child = self
            .server_child
            .as_ref()
            .context("running fixture server")?;
        child.signal(Signal::SIGSTOP)?;
        Ok(ServerPause(child))
    }

    pub fn driver(&self) -> &WebDriver {
        self.driver.as_ref().unwrap()
    }

    pub fn driver_if_active(&self) -> Option<&WebDriver> {
        self.driver.as_ref()
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn write_user_config(&self, contents: &str) -> Result<()> {
        let path = self.data_root.join("config.toml");
        fs::write(&path, contents)
            .with_context(|| format!("write E2E user settings {}", path.display()))
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

    pub async fn restart_server(&mut self) -> Result<()> {
        let mut server = self
            .server_child
            .take()
            .ok_or_else(|| anyhow!("gtl-server is not running"))?;
        terminate_child("gtl-server", &mut server)
            .await
            .context("stop gtl-server before restart")?;
        self.server_child = Some(
            start_server(&self.data_root)
                .await
                .context("start replacement gtl-server")?,
        );
        Ok(())
    }

    pub async fn finish(mut self) -> Result<()> {
        self.shutdown().await
    }

    async fn shutdown(&mut self) -> Result<()> {
        let driver_result = match self.driver.take() {
            Some(driver) => quit_driver(driver).await,
            None => Ok(()),
        };
        let child_result = match self.driver_child.as_mut() {
            Some(child) => terminate_child("tauri-driver", child).await,
            None => Ok(()),
        };
        if child_result.is_ok() {
            self.driver_child.take();
        }

        let driver_result = match (driver_result, child_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(driver_error), Ok(())) => Err(driver_error),
            (Ok(()), Err(child_error)) => Err(child_error),
            (Err(driver_error), Err(child_error)) => Err(driver_error)
                .context(format!("tauri-driver cleanup also failed: {child_error:#}")),
        };
        let server_result = match self.server_child.as_mut() {
            Some(child) => terminate_child("gtl-server", child).await,
            None => Ok(()),
        };
        if server_result.is_ok() {
            self.server_child.take();
        }
        match (driver_result, server_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(driver_error), Ok(())) => Err(driver_error),
            (Ok(()), Err(server_error)) => Err(server_error),
            (Err(driver_error), Err(server_error)) => Err(driver_error)
                .context(format!("gtl-server cleanup also failed: {server_error:#}")),
        }
    }
}

async fn start_driver_with_retries(
    data_root: &Path,
    viewer_binary: &str,
) -> Result<(WebDriver, GroupChild)> {
    let mut failure = None;
    for attempt in 1..=START_ATTEMPTS_MAX {
        match start_driver_attempt(data_root, viewer_binary, attempt).await {
            Ok(started) => return Ok(started),
            Err(error) => failure = Some(error),
        }
    }
    Err(failure.unwrap_or_else(|| anyhow!("tauri-driver did not start")))
}

async fn start_driver_attempt(
    data_root: &Path,
    viewer_binary: &str,
    attempt: usize,
) -> Result<(WebDriver, GroupChild)> {
    let ports = available_port_pair().context("select distinct driver ports")?;
    let (webdriver_port, native_driver_port) = ports.ports()?;
    let mut command = Command::new("tauri-driver");
    command
        .arg("--port")
        .arg(webdriver_port.to_string())
        .arg("--native-port")
        .arg(native_driver_port.to_string())
        .env("GIT_TOOLS_DATA_DIR", data_root);
    deny_external_proxies(&mut command);
    drop(ports);
    let mut driver_child = command
        .group_spawn()
        .with_context(|| format!("spawn tauri-driver attempt {attempt}"))?;
    let connection = timeout(
        START_ATTEMPT_TIMEOUT,
        connect_driver(webdriver_port, viewer_binary, &mut driver_child),
    )
    .await
    .map_err(|_| anyhow!("tauri-driver attempt {attempt} exceeded {START_ATTEMPT_TIMEOUT:?}"))
    .and_then(|result| result)
    .with_context(|| format!("connect to tauri-driver on attempt {attempt}"));

    match connection {
        Ok(driver) => Ok((driver, driver_child)),
        Err(error) => {
            terminate_child("tauri-driver", &mut driver_child)
                .await
                .with_context(|| format!("clean up tauri-driver attempt {attempt}"))?;
            Err(error)
        }
    }
}

async fn quit_driver(driver: WebDriver) -> Result<()> {
    wait::within(
        "quit WebDriver session",
        WEBDRIVER_OPERATION_TIMEOUT,
        async { driver.quit().await.map_err(anyhow::Error::from) },
    )
    .await
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
        if let Some(child) = self.server_child.as_mut() {
            let _ = child.kill();
        }
    }
}

#[cfg(unix)]
pub struct ServerPause<'child>(&'child GroupChild);

#[cfg(unix)]
impl Drop for ServerPause<'_> {
    fn drop(&mut self) {
        let _ = self.0.signal(Signal::SIGCONT);
    }
}

async fn start_server(data_root: &Path) -> Result<GroupChild> {
    let server_binary = required_binary("GTL_E2E_SERVER_BINARY", "gtl-server")?;
    let endpoint = data_root.join("server").join("endpoint.json");
    match fs::remove_file(&endpoint) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("remove stale server endpoint {}", endpoint.display()));
        }
    }
    let mut command = Command::new(&server_binary);
    command
        .env("GIT_TOOLS_DATA_DIR", data_root)
        .env("GIT_TOOLS_CONFIG", data_root.join("config.toml"))
        .env("RUST_LOG", "info")
        .stderr(Stdio::from(fs::File::create(
            data_root.join("server.stderr.log"),
        )?))
        .stdin(Stdio::null());
    deny_external_proxies(&mut command);
    let mut child = command.group_spawn().with_context(|| {
        format!(
            "spawn gtl-server for desktop E2E from {}",
            server_binary.display()
        )
    })?;
    let deadline = Instant::now() + SERVER_READY_TIMEOUT;
    loop {
        if endpoint.is_file() {
            return Ok(child);
        }
        if let Some(status) = child.try_wait().context("inspect gtl-server launch")? {
            bail!("gtl-server exited before endpoint publication ({status})");
        }
        if Instant::now() >= deadline {
            let error = anyhow!(
                "gtl-server did not publish {} within {SERVER_READY_TIMEOUT:?}",
                endpoint.display()
            );
            return Err(cleanup_start_failure("gtl-server", &mut child, error).await);
        }
        sleep(CONNECTION_RETRY_INTERVAL).await;
    }
}

async fn cleanup_start_failure(
    name: &str,
    child: &mut GroupChild,
    error: anyhow::Error,
) -> anyhow::Error {
    match terminate_child(name, child).await {
        Ok(()) => error,
        Err(cleanup_error) => {
            error.context(format!("{name} cleanup also failed: {cleanup_error:#}"))
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
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            break;
        }
        if driver_child
            .try_wait()
            .context("inspect tauri-driver launch")?
            .is_some()
        {
            bail!("tauri-driver exited before accepting WebDriver connections");
        }
        if Instant::now() >= deadline {
            bail!("tauri-driver did not accept connections within {START_ATTEMPT_TIMEOUT:?}");
        }
        sleep(CONNECTION_RETRY_INTERVAL).await;
    }

    let driver = WebDriver::new(&driver_url, capabilities)
        .await
        .context("create one WebDriver session after tauri-driver became reachable")?;
    driver
        .query(By::Css("button[aria-label='Refresh projects']"))
        .and_enabled()
        .and_displayed()
        .wait(wait::ASSERTION_TIMEOUT, Duration::from_millis(100))
        .first()
        .await
        .context("wait for the Projects home connection")?;
    Ok(driver)
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
    let path = required_binary("GTL_E2E_VIEWER_BINARY", "viewer")?;
    path.canonicalize()
        .with_context(|| format!("canonicalize viewer binary {}", path.display()))
        .map(|path| path.to_string_lossy().into_owned())
}

fn required_binary(environment_variable: &str, label: &str) -> Result<PathBuf> {
    env::var_os(environment_variable)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{environment_variable} is required for the {label}"))
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

async fn terminate_child(name: &str, child: &mut GroupChild) -> Result<()> {
    if wait_for_exit(child, CHILD_EXIT_GRACE_TIMEOUT).await? {
        return Ok(());
    }

    if request_group_termination(child)
        .with_context(|| format!("terminate {name} process group"))?
        && wait_for_exit(child, CHILD_TERMINATE_TIMEOUT).await?
    {
        return Ok(());
    }

    force_kill_group(child).with_context(|| format!("kill {name} process group"))?;
    if wait_for_exit(child, CLEANUP_TIMEOUT).await? {
        return Ok(());
    }

    Err(anyhow!(
        "{name} process group did not exit within {CLEANUP_TIMEOUT:?} after kill"
    ))
}

#[cfg(unix)]
fn request_group_termination(child: &GroupChild) -> std::io::Result<bool> {
    match child.signal(Signal::SIGTERM) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn request_group_termination(_: &GroupChild) -> std::io::Result<bool> {
    Ok(false)
}

fn force_kill_group(child: &mut GroupChild) -> std::io::Result<()> {
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
        path::{Path, PathBuf},
        process::Command,
        time::Duration,
    };

    use anyhow::{Context as _, Result, ensure};
    use command_group::CommandGroup;

    use super::{request_group_termination, wait_for_exit};

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn terminating_driver_reaps_its_process_group() {
        terminate_driver_process_group().await.unwrap();
    }

    #[cfg(target_os = "linux")]
    async fn terminate_driver_process_group() -> Result<()> {
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
        let child_pid = wait_for_child_pid(&child_pid_path).await;
        if child_pid.is_none() {
            let _ = child.kill();
            let _ = wait_for_exit(&mut child, Duration::from_secs(1)).await;
        }
        let child_pid = child_pid.context("child pid was not written")?;

        let termination_requested = request_group_termination(&child);
        if !matches!(&termination_requested, Ok(true)) {
            let _ = child.kill();
            let _ = wait_for_exit(&mut child, Duration::from_secs(1)).await;
        }
        ensure!(
            termination_requested?,
            "process-group termination was not requested"
        );
        let exited = wait_for_process_exit(child_pid, Duration::from_secs(1)).await;
        if !exited {
            let _ = child.kill();
        }
        let reaped = wait_for_exit(&mut child, Duration::from_secs(1)).await;

        ensure!(
            exited,
            "child process remained live after group termination"
        );
        ensure!(reaped?, "driver process group was not reaped");
        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn wait_for_child_pid(path: &Path) -> Option<u32> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        while tokio::time::Instant::now() < deadline {
            match read_child_pid(path) {
                Some(pid) => return Some(pid),
                None => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
        read_child_pid(path)
    }

    #[cfg(target_os = "linux")]
    fn read_child_pid(path: &Path) -> Option<u32> {
        fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    #[cfg(target_os = "linux")]
    async fn wait_for_process_exit(pid: u32, duration: Duration) -> bool {
        let process_path = PathBuf::from(format!("/proc/{pid}"));
        let deadline = tokio::time::Instant::now() + duration;
        while process_path.exists() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        !process_path.exists()
    }
}
