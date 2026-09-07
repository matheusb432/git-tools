use std::{
    fs::{self, File},
    io::Read as _,
    net::{Ipv4Addr, SocketAddrV4, TcpListener},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use command_group::{CommandGroup, GroupChild, Signal, UnixChildExt};
use serde::Deserialize;

use super::{HostCargoEnvironment, IsolatedEnv, Sandbox};

mod command;

const INSTALL_ATTEMPTS_MAX: u32 = 4;
const DRIVER_CACHE_DIR: &str = ".cache/playwright-driver";
const BROWSER_CACHE_DIR: &str = "browsers";
const INSTALL_TIMEOUT: Duration = Duration::from_mins(3);
const TEST_TIMEOUT: Duration = Duration::from_mins(2);
const COMPONENT_PREVIEW_READY_TIMEOUT: Duration = Duration::from_mins(5);
const COMPONENT_PREVIEW_POLL_INTERVAL: Duration = Duration::from_millis(100);
const COMPONENT_PREVIEW_URL_ENVIRONMENT_VARIABLE: &str = "GTL_COMPONENT_PREVIEW_URL";

struct ComponentPreviewServer {
    child: GroupChild,
}

struct CachePaths {
    driver: PathBuf,
    browsers: PathBuf,
}

fn cache_paths(repository_root: &Path) -> CachePaths {
    let driver = repository_root.join(DRIVER_CACHE_DIR);
    let browsers = driver.join(BROWSER_CACHE_DIR);
    CachePaths { driver, browsers }
}

pub(super) fn run(
    sandbox: &Sandbox,
    environment: &IsolatedEnv,
    host_environment: &HostCargoEnvironment,
) -> Result<()> {
    let repository_root = std::env::current_dir().context("resolve repository root")?;
    let cache = cache_paths(&repository_root);
    fs::create_dir_all(&cache.driver)
        .with_context(|| format!("create Playwright driver cache {}", cache.driver.display()))?;
    fs::create_dir_all(&cache.browsers).with_context(|| {
        format!(
            "create Playwright browser cache {}",
            cache.browsers.display()
        )
    })?;

    let mut environment = environment.clone();
    environment.set("PLAYWRIGHT_DRIVER_CACHE_DIR", &cache.driver);
    environment.set("PLAYWRIGHT_BROWSERS_PATH", &cache.browsers);
    environment.set("GTL_E2E_CLI_BINARY", &sandbox.cli_binary);

    install_browsers(sandbox, &environment, host_environment)?;
    let (preview_server, preview_url) =
        ComponentPreviewServer::start(sandbox, &environment, host_environment)?;
    environment.set(COMPONENT_PREVIEW_URL_ENVIRONMENT_VARIABLE, preview_url);
    let result = command::run(
        sandbox,
        &environment,
        host_environment,
        "playwright-artifact.log",
        &[
            "test",
            "-p",
            "gtl-browser-e2e",
            "--features",
            "e2e",
            "--test",
            "browser",
            "--",
            "--test-threads",
            "1",
        ],
        "run Playwright browser E2E",
        TEST_TIMEOUT,
    );
    drop(preview_server);
    result
}

impl ComponentPreviewServer {
    fn start(
        sandbox: &Sandbox,
        environment: &IsolatedEnv,
        host_environment: &HostCargoEnvironment,
    ) -> Result<(Self, String)> {
        let port = available_loopback_port()?;
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        let log_path = sandbox.logs.join("component-preview.log");
        let stderr = File::create(&log_path)
            .with_context(|| format!("create component preview log {}", log_path.display()))?;
        let ready_path = sandbox.logs.join("component-preview-ready.jsonl");
        let stdout =
            File::create(&ready_path).context("create component preview readiness output")?;
        let mut command = Command::new("cargo");
        command
            .args([
                "run",
                "--quiet",
                "--locked",
                "--manifest-path",
                "../../shared-libs/dx-story/Cargo.toml",
                "-p",
                "dx-story-cli",
                "--",
                "serve",
                "--no-watch",
                "--ready-json",
                "--open",
                "no",
                "--port",
                &port.to_string(),
            ])
            .current_dir(".")
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        environment.apply_cargo(&mut command, host_environment);
        let child = command
            .group_spawn()
            .context("start Dioxus component preview server")?;
        let mut server = Self { child };
        wait_for_component_preview(&mut server.child, address, &ready_path, &log_path)?;
        Ok((server, format!("http://{address}")))
    }
}

fn wait_for_component_preview(
    child: &mut GroupChild,
    address: SocketAddrV4,
    ready_path: &Path,
    log_path: &Path,
) -> Result<()> {
    let deadline = Instant::now() + COMPONENT_PREVIEW_READY_TIMEOUT;
    loop {
        if let Some(status) = child
            .try_wait()
            .context("poll Dioxus component preview server")?
        {
            let log = fs::read_to_string(log_path).unwrap_or_default();
            bail!(
                "Dioxus component preview exited before readiness (exit {}): {log}",
                status.code().unwrap_or(-1)
            );
        }
        if component_preview_ready(ready_path, address)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let log = fs::read_to_string(log_path).unwrap_or_default();
            bail!(
                "Dioxus component preview did not become ready within {} seconds: {log}",
                COMPONENT_PREVIEW_READY_TIMEOUT.as_secs()
            );
        }
        thread::sleep(COMPONENT_PREVIEW_POLL_INTERVAL);
    }
}

impl Drop for ComponentPreviewServer {
    fn drop(&mut self) {
        let _ = self.child.signal(Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(6);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            thread::sleep(COMPONENT_PREVIEW_POLL_INTERVAL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn available_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .context("reserve component preview loopback port")?;
    listener
        .local_addr()
        .map(|address| address.port())
        .context("resolve component preview loopback port")
}

#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
enum PreviewEvent {
    Ready { url: String },
}

fn component_preview_ready(path: &Path, address: SocketAddrV4) -> Result<bool> {
    let mut output = String::new();
    File::open(path)?.take(4096).read_to_string(&mut output)?;
    let Some(line) = output.lines().next() else {
        return Ok(false);
    };
    let event = match serde_json::from_str::<PreviewEvent>(line) {
        Ok(event) => event,
        Err(error) if error.is_eof() => return Ok(false),
        Err(error) => return Err(error).context("decode dx-story readiness event"),
    };
    let PreviewEvent::Ready { url } = event;
    anyhow::ensure!(
        url == format!("http://{address}"),
        "unexpected component preview URL: {url}"
    );
    Ok(true)
}

fn install_browsers(
    sandbox: &Sandbox,
    environment: &IsolatedEnv,
    host_environment: &HostCargoEnvironment,
) -> Result<()> {
    let mut last_error = None;
    for attempt in 1..=INSTALL_ATTEMPTS_MAX {
        match command::run(
            sandbox,
            environment,
            host_environment,
            "playwright-install.log",
            &[
                "run",
                "--quiet",
                "-p",
                "gtl-browser-e2e",
                "--features",
                "e2e",
                "--bin",
                "install-browsers",
            ],
            "install Chromium for Playwright E2E",
            INSTALL_TIMEOUT,
        ) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some((attempt, error)),
        }
    }
    let (attempt, error) = last_error.context("Playwright installer made no attempts")?;
    bail!("install Chromium for Playwright E2E failed after {attempt} attempts: {error:#}")
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum InstallOutcome {
    Installed { attempt: u32 },
    Failed,
}

#[cfg(test)]
fn decide_install(install: &impl Fn() -> bool, attempts_max: u32) -> InstallOutcome {
    for attempt in 1..=attempts_max {
        if install() {
            return InstallOutcome::Installed { attempt };
        }
    }
    InstallOutcome::Failed
}

#[cfg(test)]
mod tests;
