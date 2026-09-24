//! Hermetic viewer suites. On Linux, every desktop test process owns an isolated
//! Xvfb/Openbox/stalonetray/D-Bus session: the native suite uses one, and the `WebDriver`
//! journeys run in parallel with one session per Nextest slot.

use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use command_group::{CommandGroup, GroupChild};
use serde::{Deserialize, Serialize};

use super::{build, status_notifier::StatusNotifierWatcher};
use crate::cli::{BuildTarget, DesktopE2eSuite};

mod native;
mod playwright;
mod stable_runner;

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const WINDOW_TITLE_PATTERN: &str = "^git-tools diff viewer$";
const EVIDENCE_OUTPUT_PATH_ENVIRONMENT_VARIABLE: &str = "GTL_E2E_EVIDENCE_OUTPUT_PATH";
const EVIDENCES_OUTPUT_PATH_ENVIRONMENT_VARIABLE: &str = "TEST_EVIDENCES_OUTPUT_PATH";
const EVIDENCE_OUTPUT_PATH_DEFAULT: &str = ".artifacts/e2e";
const DESKTOP_SLOTS_ENVIRONMENT_VARIABLE: &str = "GTL_E2E_DESKTOP_SLOTS";
const NEXTEST_SLOT_ENVIRONMENT_VARIABLE: &str = "NEXTEST_TEST_GLOBAL_SLOT";
const JOURNEY_SLOT_COUNT: usize = 4;
const CAPTURED_COMMAND_BYTES_MAX: usize = 8 * 1024;
const SCROLL_BENCHMARK_OUTPUT_BYTES_MAX: usize = 8 * 1024 * 1024;
const SCROLL_BENCHMARK_TEST_ARGUMENTS: &[&str] = &[
    "test",
    "-p",
    "gtl-desktop-e2e",
    "--features",
    "e2e",
    "--test",
    "viewer",
    "--",
    "desktop_scroll_baseline::production_viewer_scrolls_large_diff_workloads",
    "--exact",
    "--ignored",
    "--nocapture",
    "--test-threads",
    "1",
];
const SCROLL_BENCHMARK_ENVIRONMENT_VARIABLE_NAMES: &[&str] = &[
    "GTL_DESKTOP_SCROLL_REPORT_PATH",
    "GTL_DESKTOP_SCROLL_LAUNCHES",
    "GTL_DESKTOP_SCROLL_INTERACTION_SAMPLES",
    "GTL_DESKTOP_SCROLL_SOURCE_COMMIT",
    "GTL_DESKTOP_SCROLL_INVOCATION",
    "GTL_DESKTOP_SCROLL_CPU_QUOTA_PERCENT",
    "GTL_DESKTOP_SCROLL_MEMORY_MAX_BYTES",
    "GTL_DESKTOP_SCROLL_MEMORY_SWAP_MAX_BYTES",
    "GTL_DESKTOP_SCROLL_TASKS_MAX",
    "GTL_DESKTOP_SCROLL_PROCESS_NICENESS",
    "GTL_DESKTOP_SCROLL_CARGO_JOBS_MAX",
    "GTL_DESKTOP_SCROLL_RAYON_THREADS_MAX",
    "GTL_DESKTOP_SCROLL_WALL_TIME_MINUTES",
    "GTL_DESKTOP_SCROLL_TERMINATION_GRACE_SECONDS",
];
const HOST_ENVIRONMENT_VARIABLE_NAMES: &[&str] =
    &["PATH", "SystemRoot", "WINDIR", "PATHEXT", "COMSPEC"];
#[cfg(not(windows))]
const HOST_HOME_ENVIRONMENT_VARIABLE: &str = "HOME";
#[cfg(windows)]
const HOST_HOME_ENVIRONMENT_VARIABLE: &str = "USERPROFILE";

#[derive(Debug)]
struct Sandbox {
    _root: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
    runtime: PathBuf,
    temp: PathBuf,
    fixtures: PathBuf,
    logs: PathBuf,
    native_data: PathBuf,
    dom_data: PathBuf,
    browser_data: PathBuf,
    cli_binary: PathBuf,
    server_binary: PathBuf,
    viewer_binary: PathBuf,
    cargo_runner_config: PathBuf,
    evidence_root: PathBuf,
    success_evidence_requested: bool,
}

impl Sandbox {
    fn create() -> Result<Self> {
        let guard = tempfile::Builder::new()
            .prefix("gtl-viewer-e2e-")
            .tempdir()
            .context("create viewer E2E sandbox")?;
        let root = guard.path().to_path_buf();
        let repository_root = env::current_dir().context("resolve repository root")?;
        let requested_evidence_root = env::var_os(EVIDENCES_OUTPUT_PATH_ENVIRONMENT_VARIABLE);
        let success_evidence_requested = requested_evidence_root.is_some();
        let evidence_root = requested_evidence_root.map_or_else(
            || PathBuf::from(EVIDENCE_OUTPUT_PATH_DEFAULT),
            PathBuf::from,
        );
        let evidence_root = if evidence_root.is_absolute() {
            evidence_root
        } else {
            repository_root.join(evidence_root)
        };
        let sandbox = Self {
            _root: guard,
            home: root.join("home"),
            config: root.join("xdg/config"),
            data: root.join("xdg/data"),
            cache: root.join("xdg/cache"),
            runtime: root.join("xdg/runtime"),
            temp: root.join("tmp"),
            fixtures: root.join("fixtures"),
            logs: root.join("logs"),
            native_data: root.join("git-tools/native"),
            dom_data: root.join("git-tools/dom"),
            browser_data: root.join("git-tools/browser"),
            cli_binary: release_binary("git-tools")?,
            server_binary: release_binary("gtl-server")?,
            viewer_binary: release_binary("gtl-viewer")?,
            cargo_runner_config: root.join("cargo-runner.toml"),
            evidence_root,
            success_evidence_requested,
            root,
        };
        for path in [
            &sandbox.home,
            &sandbox.config,
            &sandbox.data,
            &sandbox.cache,
            &sandbox.runtime,
            &sandbox.temp,
            &sandbox.fixtures,
            &sandbox.logs,
            &sandbox.native_data,
            &sandbox.dom_data,
            &sandbox.browser_data,
        ] {
            fs::create_dir_all(path)
                .with_context(|| format!("create sandbox directory {}", path.display()))?;
        }
        set_runtime_directory_permissions(&sandbox.runtime)?;
        let runtime_runner = sandbox
            .root
            .join(format!("xtask-e2e-runner{}", env::consts::EXE_SUFFIX));
        stable_runner::copy_current_executable(&runtime_runner)
            .context("copy stable E2E runtime runner")?;
        fs::write(
            &sandbox.cargo_runner_config,
            cargo_runner_config(&runtime_runner),
        )
        .context("write E2E Cargo runner configuration")?;
        Ok(sandbox)
    }

    fn environment(&self, data_root: &Path) -> IsolatedEnv {
        let settings_path = self.config.join("git-tools.toml");
        let mut pairs = vec![
            ("HOME", self.home.as_os_str()),
            ("XDG_CONFIG_HOME", self.config.as_os_str()),
            ("XDG_DATA_HOME", self.data.as_os_str()),
            ("XDG_CACHE_HOME", self.cache.as_os_str()),
            ("XDG_RUNTIME_DIR", self.runtime.as_os_str()),
            ("TMPDIR", self.temp.as_os_str()),
            ("TEMP", self.temp.as_os_str()),
            ("TMP", self.temp.as_os_str()),
            ("USERPROFILE", self.home.as_os_str()),
            ("LOCALAPPDATA", self.data.as_os_str()),
            ("APPDATA", self.config.as_os_str()),
            ("GIT_TOOLS_DATA_DIR", data_root.as_os_str()),
            ("GIT_TOOLS_CONFIG", settings_path.as_os_str()),
            ("GTL_E2E_DATA_ROOT", data_root.as_os_str()),
            ("GTL_E2E_FIXTURE_ROOT", self.fixtures.as_os_str()),
            ("GTL_E2E_CLI_BINARY", self.cli_binary.as_os_str()),
            ("GTL_E2E_SERVER_BINARY", self.server_binary.as_os_str()),
            ("GTL_E2E_VIEWER_BINARY", self.viewer_binary.as_os_str()),
            (
                EVIDENCE_OUTPUT_PATH_ENVIRONMENT_VARIABLE,
                self.evidence_root.as_os_str(),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value.to_os_string()))
        .collect::<Vec<_>>();
        pairs.extend(
            HOST_ENVIRONMENT_VARIABLE_NAMES
                .iter()
                .copied()
                .filter_map(|name| {
                    std::env::var_os(name).map(|value| (OsString::from(name), value))
                }),
        );
        if self.success_evidence_requested {
            pairs.push((
                OsString::from(EVIDENCES_OUTPUT_PATH_ENVIRONMENT_VARIABLE),
                self.evidence_root.as_os_str().to_os_string(),
            ));
        }
        IsolatedEnv { pairs }
    }
}

fn set_runtime_directory_permissions(runtime: &Path) -> Result<()> {
    if env::consts::OS != "linux" {
        return Ok(());
    }

    let status = Command::new("chmod")
        .arg("700")
        .arg(runtime)
        .status()
        .context("set XDG_RUNTIME_DIR permissions")?;
    ensure!(status.success(), "chmod 700 for XDG_RUNTIME_DIR failed");
    Ok(())
}

#[derive(Debug, Clone)]
struct IsolatedEnv {
    pairs: Vec<(OsString, OsString)>,
}

impl IsolatedEnv {
    pub(super) fn set(&mut self, name: &str, value: impl AsRef<OsStr>) {
        let name = OsString::from(name);
        self.pairs.retain(|(current, _)| current != &name);
        self.pairs.push((name, value.as_ref().to_os_string()));
    }

    fn get(&self, name: &str) -> Option<&OsStr> {
        self.pairs
            .iter()
            .find(|(current, _)| current == name)
            .map(|(_, value)| value.as_os_str())
    }

    pub(super) fn apply(&self, command: &mut Command) {
        command.env_clear().envs(self.pairs.iter().cloned());
    }

    pub(super) fn apply_cargo(
        &self,
        command: &mut Command,
        host_environment: &HostCargoEnvironment,
    ) {
        self.apply(command);
        host_environment.apply(command);
    }
}

#[derive(Debug)]
pub(super) struct HostCargoEnvironment {
    pairs: Vec<(OsString, OsString)>,
}

impl HostCargoEnvironment {
    pub(super) fn capture() -> Result<Self> {
        Self::from_environment(|name| env::var_os(name))
    }

    fn from_environment(mut value: impl FnMut(&str) -> Option<OsString>) -> Result<Self> {
        Self::from_environment_with_home_variable(&mut value, HOST_HOME_ENVIRONMENT_VARIABLE)
    }

    fn from_environment_with_home_variable(
        mut value: impl FnMut(&str) -> Option<OsString>,
        home_variable: &str,
    ) -> Result<Self> {
        let host_home = value(home_variable);
        let cargo_home = value("CARGO_HOME")
            .or_else(|| {
                host_home
                    .as_ref()
                    .map(|home| Path::new(home).join(".cargo").into())
            })
            .with_context(|| {
                format!("resolve host Cargo home from CARGO_HOME or {home_variable}")
            })?;
        let rustup_home = value("RUSTUP_HOME")
            .or_else(|| {
                host_home
                    .as_ref()
                    .map(|home| Path::new(home).join(".rustup").into())
            })
            .with_context(|| {
                format!("resolve host rustup home from RUSTUP_HOME or {home_variable}")
            })?;
        let mut pairs = vec![
            (OsString::from("CARGO_HOME"), cargo_home),
            (OsString::from("RUSTUP_HOME"), rustup_home),
        ];
        if let Some(agent_socket) = value("SSH_AUTH_SOCK") {
            pairs.push((OsString::from("SSH_AUTH_SOCK"), agent_socket));
        }
        Ok(Self { pairs })
    }

    fn apply(&self, command: &mut Command) {
        command.envs(self.pairs.iter().cloned());
    }
}

#[derive(Debug)]
struct ManagedChild {
    name: &'static str,
    child: Option<GroupChild>,
}

impl ManagedChild {
    fn spawn(
        name: &'static str,
        program: &str,
        args: &[&str],
        env: &IsolatedEnv,
        cwd: &Path,
        log: &Path,
    ) -> Result<Self> {
        let stdout =
            File::create(log).with_context(|| format!("create {name} log {}", log.display()))?;
        let stderr = stdout.try_clone().context("clone child log file")?;
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        env.apply(&mut command);
        let child = command
            .group_spawn()
            .with_context(|| format!("start {name}"))?;
        Ok(Self {
            name,
            child: Some(child),
        })
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if child.try_wait().ok().flatten().is_none()
            && let Err(error) = child.kill()
        {
            eprintln!("desktop-e2e: failed to kill {}: {error}", self.name);
        }
        if let Err(error) = child.wait() {
            eprintln!("desktop-e2e: failed to reap {}: {error}", self.name);
        }
    }
}

pub(crate) fn run_scroll_benchmark() -> Result<()> {
    if std::env::consts::OS != "linux" {
        bail!("the production desktop scroll benchmark requires Linux WebKit");
    }
    build::run(BuildTarget::Both)?;

    let sandbox = Sandbox::create()?;
    clear_evidence_outcomes(&sandbox.evidence_root, sandbox.success_evidence_requested)?;
    let environment = scroll_benchmark_environment(&sandbox)?;
    let result = run_linux_session(&sandbox, environment, |environment| {
        let host_environment = HostCargoEnvironment::capture()?;
        run_scroll_benchmark_phase(&sandbox, environment, &host_environment)
    });
    let logs_result = preserve_logs(&sandbox, "desktop-scroll-benchmark");
    result?;
    logs_result
}

pub(crate) fn run_runtime(executable: &Path, arguments: &[OsString]) -> Result<()> {
    let slot = DesktopSlot::assigned()?;
    let mut command = runtime_command(executable, arguments, slot.as_ref());
    let status = command
        .status()
        .with_context(|| format!("run isolated E2E executable {}", executable.display()))?;
    if !status.success() {
        bail!(
            "isolated E2E executable failed (exit {})",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

fn runtime_command(
    executable: &Path,
    arguments: &[OsString],
    slot: Option<&DesktopSlot>,
) -> Command {
    let mut command = Command::new(executable);
    command.args(arguments).env("GTL_E2E_RUNTIME_ISOLATED", "1");
    for name in ["CARGO_HOME", "RUSTUP_HOME", "SSH_AUTH_SOCK"] {
        command.env_remove(name);
    }
    if let Some(slot) = slot {
        command
            .env("DISPLAY", &slot.display)
            .env("DBUS_SESSION_BUS_ADDRESS", &slot.dbus_session_bus_address);
    }
    command
}

/// The desktop session a Nextest slot's test process uses.
#[derive(Debug, Serialize, Deserialize)]
struct DesktopSlot {
    display: String,
    dbus_session_bus_address: String,
}

impl DesktopSlot {
    fn path(directory: &Path, slot: &OsStr) -> PathBuf {
        directory.join(slot).with_extension("json")
    }

    /// Reads the slot Nextest assigned to this test process, if the journey run published slots.
    fn assigned() -> Result<Option<Self>> {
        let (Some(directory), Some(slot)) = (
            env::var_os(DESKTOP_SLOTS_ENVIRONMENT_VARIABLE),
            env::var_os(NEXTEST_SLOT_ENVIRONMENT_VARIABLE),
        ) else {
            return Ok(None);
        };
        let path = Self::path(Path::new(&directory), &slot);
        let contents =
            fs::read(&path).with_context(|| format!("read desktop slot {}", path.display()))?;
        serde_json::from_slice(&contents)
            .with_context(|| format!("decode desktop slot {}", path.display()))
            .map(Some)
    }

    fn write(&self, directory: &Path, slot: usize) -> Result<()> {
        let path = Self::path(directory, OsStr::new(&slot.to_string()));
        fs::write(&path, serde_json::to_vec(self)?)
            .with_context(|| format!("write desktop slot {}", path.display()))
    }
}

fn cargo_runner_config(executable: &Path) -> String {
    format!(
        "[target.'cfg(all())']\nrunner = [{:?}, \"e2e-runtime-worker\"]\n",
        executable.to_string_lossy()
    )
}

/// Build the release artifacts and run one hermetic desktop E2E suite.
pub(crate) fn run(suite: DesktopE2eSuite, nextest_arguments: &[OsString]) -> Result<()> {
    ensure!(
        suite == DesktopE2eSuite::Journeys || nextest_arguments.is_empty(),
        "only the journey suite accepts Nextest arguments"
    );
    build::run(BuildTarget::Both)?;

    let sandbox = Sandbox::create()?;
    clear_evidence_outcomes(&sandbox.evidence_root, sandbox.success_evidence_requested)?;
    let result = match (suite, std::env::consts::OS) {
        (DesktopE2eSuite::Journeys, "linux") => run_journeys(&sandbox, nextest_arguments),
        (DesktopE2eSuite::Native, "linux") => run_linux_session(
            &sandbox,
            sandbox.environment(&sandbox.native_data),
            |environment| native::run(&sandbox, environment),
        ),
        (DesktopE2eSuite::Browser, "linux" | "windows") => run_browser(&sandbox),
        (suite, unsupported) => {
            bail!("the {suite:?} desktop E2E suite is not configured for {unsupported}")
        }
    };
    let logs_result = preserve_logs(&sandbox, suite.log_directory_name());
    result?;
    logs_result
}

impl DesktopE2eSuite {
    const fn log_directory_name(self) -> &'static str {
        match self {
            Self::Journeys => "desktop-e2e-journeys",
            Self::Native => "desktop-e2e-native",
            Self::Browser => "desktop-e2e-browser",
        }
    }
}

/// Runs the `WebDriver` journeys through Nextest, one desktop session per test slot.
fn run_journeys(sandbox: &Sandbox, nextest_arguments: &[OsString]) -> Result<()> {
    let mut environment = sandbox.environment(&sandbox.dom_data);
    environment.set("NO_AT_BRIDGE", "1");
    let slot_directory = sandbox.root.join("desktop-slots");
    fs::create_dir_all(&slot_directory)
        .with_context(|| format!("create desktop slots {}", slot_directory.display()))?;
    let mut sessions = Vec::with_capacity(JOURNEY_SLOT_COUNT);
    for slot in 0..JOURNEY_SLOT_COUNT {
        let session = DesktopSession::start(sandbox, &format!("slot-{slot}"), environment.clone())?;
        session.slot()?.write(&slot_directory, slot)?;
        sessions.push(session);
    }
    environment.set(DESKTOP_SLOTS_ENVIRONMENT_VARIABLE, &slot_directory);

    let host_environment = HostCargoEnvironment::capture()?;
    let mut command = Command::new("cargo");
    command
        .args(["nextest", "run", "--config"])
        .arg(&sandbox.cargo_runner_config)
        .args([
            "--locked",
            "-p",
            "gtl-desktop-e2e",
            "--features",
            "e2e",
            "--test",
            "viewer",
            "--profile",
            "e2e",
            "--test-threads",
        ])
        .arg(JOURNEY_SLOT_COUNT.to_string())
        .args(nextest_arguments)
        .current_dir(".")
        .stdin(Stdio::null());
    seed_hostile_git_environment(&mut command, &sandbox.root);
    environment.apply_cargo(&mut command, &host_environment);
    let status = command.status().context("run desktop E2E journeys")?;
    if !status.success() {
        for session in &sessions {
            session.capture_diagnostics(sandbox);
        }
        bail!("desktop E2E journeys failed");
    }
    Ok(())
}

fn run_linux_session<T>(
    sandbox: &Sandbox,
    environment: IsolatedEnv,
    operation: impl FnOnce(&IsolatedEnv) -> Result<T>,
) -> Result<T> {
    let session = DesktopSession::start(sandbox, "desktop", environment)?;
    let result = operation(&session.environment);
    if result.is_err() {
        session.capture_diagnostics(sandbox);
    }
    result
}

/// One isolated X display with its window manager, tray, and private D-Bus session.
///
/// Fields drop in declaration order, which stops the processes in reverse start order.
struct DesktopSession {
    label: String,
    environment: IsolatedEnv,
    _tray: ManagedChild,
    _openbox: ManagedChild,
    _tray_watcher: StatusNotifierWatcher,
    _dbus: ManagedChild,
    _xvfb: ManagedChild,
}

impl DesktopSession {
    fn start(sandbox: &Sandbox, label: &str, mut environment: IsolatedEnv) -> Result<Self> {
        let display = available_display(90..190)
            .context("no free isolated X display number in the 90..190 range")?;
        let display_value = format!(":{display}");
        environment.set("DISPLAY", &display_value);
        environment.set("NO_AT_BRIDGE", "1");

        let xvfb = ManagedChild::spawn(
            "Xvfb",
            "Xvfb",
            &[
                &display_value,
                "-screen",
                "0",
                "1280x900x24",
                "-nolisten",
                "tcp",
                "-noreset",
            ],
            &environment,
            &sandbox.root,
            &sandbox.logs.join(format!("{label}-xvfb.log")),
        )?;
        retry("Xvfb readiness", READY_TIMEOUT, || {
            command_success(
                &environment,
                "xdpyinfo",
                &["-display", &display_value],
                &sandbox.root,
            )
        })?;

        let (dbus, tray_watcher) = start_private_dbus(sandbox, label, &mut environment)?;

        let openbox = ManagedChild::spawn(
            "Openbox",
            "openbox",
            &["--sm-disable"],
            &environment,
            &sandbox.root,
            &sandbox.logs.join(format!("{label}-openbox.log")),
        )?;
        retry("Openbox readiness", READY_TIMEOUT, || {
            output(
                &environment,
                "xprop",
                &["-root", "_NET_SUPPORTING_WM_CHECK"],
                &sandbox.root,
            )
            .is_ok_and(|result| result.status.success())
        })?;

        let tray = ManagedChild::spawn(
            "stalonetray",
            "stalonetray",
            &[
                "--geometry",
                "8x1+0+0",
                "--icon-size",
                "24",
                "--window-type",
                "dock",
                "--skip-taskbar",
                "--no-shrink",
            ],
            &environment,
            &sandbox.root,
            &sandbox.logs.join(format!("{label}-stalonetray.log")),
        )?;
        retry("stalonetray readiness", READY_TIMEOUT, || {
            find_window(&environment, "^stalonetray$").is_ok()
        })?;

        Ok(Self {
            label: label.to_owned(),
            environment,
            _tray: tray,
            _openbox: openbox,
            _tray_watcher: tray_watcher,
            _dbus: dbus,
            _xvfb: xvfb,
        })
    }

    fn slot(&self) -> Result<DesktopSlot> {
        let value = |name| {
            self.environment
                .get(name)
                .and_then(OsStr::to_str)
                .map(str::to_owned)
                .with_context(|| format!("desktop session {} has no {name}", self.label))
        };
        Ok(DesktopSlot {
            display: value("DISPLAY")?,
            dbus_session_bus_address: value("DBUS_SESSION_BUS_ADDRESS")?,
        })
    }

    fn capture_diagnostics(&self, sandbox: &Sandbox) {
        for (name, program, args) in [
            ("xprop-root.log", "xprop", vec!["-root"]),
            ("xwininfo-root.log", "xwininfo", vec!["-root", "-tree"]),
        ] {
            if let Ok(result) = output(&self.environment, program, &args, &sandbox.root) {
                let mut bytes = result.stdout;
                bytes.extend_from_slice(&result.stderr);
                let _ = fs::write(sandbox.logs.join(format!("{}-{name}", self.label)), bytes);
            }
        }
    }
}

fn start_private_dbus(
    sandbox: &Sandbox,
    label: &str,
    env: &mut IsolatedEnv,
) -> Result<(ManagedChild, StatusNotifierWatcher)> {
    let address_file = sandbox.root.join(format!("{label}-dbus-address"));
    let script = "printf '%s' \"$DBUS_SESSION_BUS_ADDRESS\" > \"$1\"; exec sleep 2147483647";
    let child = ManagedChild::spawn(
        "private D-Bus session",
        "dbus-run-session",
        &[
            "--",
            "bash",
            "-c",
            script,
            "desktop-e2e",
            address_file.to_string_lossy().as_ref(),
        ],
        env,
        &sandbox.root,
        &sandbox.logs.join(format!("{label}-dbus.log")),
    )?;
    retry("private D-Bus address", READY_TIMEOUT, || {
        fs::read_to_string(&address_file).is_ok_and(|value| !value.trim().is_empty())
    })?;
    let address = fs::read_to_string(address_file)?;
    let address = address.trim();
    env.set("DBUS_SESSION_BUS_ADDRESS", address);
    let watcher = StatusNotifierWatcher::start(address)?;
    Ok((child, watcher))
}

fn run_scroll_benchmark_phase(
    sandbox: &Sandbox,
    env: &IsolatedEnv,
    host_environment: &HostCargoEnvironment,
) -> Result<()> {
    let log = sandbox.logs.join("desktop-scroll-benchmark.log");
    let mut command = Command::new("cargo");
    command
        .arg("--config")
        .arg(&sandbox.cargo_runner_config)
        .args(SCROLL_BENCHMARK_TEST_ARGUMENTS)
        .current_dir(".");
    seed_hostile_git_environment(&mut command, &sandbox.root);
    env.apply_cargo(&mut command, host_environment);
    let result = command
        .output()
        .context("run the desktop scroll benchmark")?;
    ensure_captured_output_bound(
        &result,
        SCROLL_BENCHMARK_OUTPUT_BYTES_MAX,
        "desktop scroll benchmark",
    )?;
    let mut bytes = result.stdout;
    bytes.extend_from_slice(&result.stderr);
    fs::write(&log, &bytes)?;
    if !result.status.success() {
        std::io::stderr().write_all(&bytes)?;
        bail!("desktop scroll benchmark failed");
    }
    Ok(())
}

fn run_browser(sandbox: &Sandbox) -> Result<()> {
    let host_environment = HostCargoEnvironment::capture()?;
    let browser_environment = sandbox.environment(&sandbox.browser_data);
    let _server = start_server(
        sandbox,
        &browser_environment,
        &sandbox.browser_data,
        "browser gtl-server",
        "browser-server.log",
    )?;
    playwright::run(sandbox, &browser_environment, &host_environment)
}

fn scroll_benchmark_environment(sandbox: &Sandbox) -> Result<IsolatedEnv> {
    let mut environment = sandbox.environment(&sandbox.dom_data);
    for name in SCROLL_BENCHMARK_ENVIRONMENT_VARIABLE_NAMES {
        let value = env::var_os(name)
            .with_context(|| format!("{name} is required by the desktop scroll worker"))?;
        environment.set(name, value);
    }
    for (name, program, arguments) in [
        (
            "GTL_DESKTOP_SCROLL_RUSTC_VERSION",
            "rustc",
            &["--version"][..],
        ),
        (
            "GTL_DESKTOP_SCROLL_CARGO_VERSION",
            "cargo",
            &["--version"][..],
        ),
        ("GTL_DESKTOP_SCROLL_GIT_VERSION", "git", &["--version"][..]),
        (
            "GTL_DESKTOP_SCROLL_TAURI_DRIVER_VERSION",
            "mise",
            &["current", "cargo:tauri-driver"][..],
        ),
        (
            "GTL_DESKTOP_SCROLL_WEBKITGTK_VERSION",
            "pkg-config",
            &["--modversion", "webkit2gtk-4.1"][..],
        ),
    ] {
        environment.set(name, captured_command_line(program, arguments)?);
    }
    Ok(environment)
}

fn captured_command_line(program: &str, arguments: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .with_context(|| format!("capture {program} {}", arguments.join(" ")))?;
    ensure_captured_output_bound(&output, CAPTURED_COMMAND_BYTES_MAX, program)?;
    if !output.status.success() {
        bail!(
            "{program} {} failed (exit {}): {}{}",
            arguments.join(" "),
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let value = String::from_utf8(output.stdout)
        .with_context(|| format!("decode {program} version output"))?;
    let value = value.trim();
    ensure!(!value.is_empty(), "{program} returned an empty version");
    Ok(value.to_owned())
}

fn ensure_captured_output_bound(output: &Output, maximum: usize, label: &str) -> Result<()> {
    ensure!(
        output.stdout.len() <= maximum && output.stderr.len() <= maximum,
        "{label} output exceeded {maximum} bytes per stream"
    );
    Ok(())
}

fn start_server(
    sandbox: &Sandbox,
    environment: &IsolatedEnv,
    data_root: &Path,
    name: &'static str,
    log_name: &str,
) -> Result<ManagedChild> {
    let endpoint = gtl_local_transport::LocalEndpoint::from_root(data_root)
        .context("resolve browser gtl-server endpoint")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build browser gtl-server readiness runtime")?;
    let server = ManagedChild::spawn(
        name,
        sandbox.server_binary.to_string_lossy().as_ref(),
        &[],
        environment,
        &sandbox.root,
        &sandbox.logs.join(log_name),
    )?;
    retry("gtl-server readiness", READY_TIMEOUT, || {
        runtime
            .block_on(gtl_client::GtlClient::connect(&endpoint))
            .is_ok()
    })?;
    Ok(server)
}

fn clear_evidence_outcomes(evidence_root: &Path, success_evidence_requested: bool) -> Result<()> {
    let outcomes: &[&str] = if success_evidence_requested {
        &["success", "fail"]
    } else {
        &["fail"]
    };
    for outcome in outcomes {
        let outcome_path = evidence_root.join(outcome);
        if outcome_path.exists() {
            fs::remove_dir_all(&outcome_path)
                .with_context(|| format!("clear stale evidence {}", outcome_path.display()))?;
        }
    }
    Ok(())
}

fn seed_hostile_git_environment(command: &mut Command, sandbox_root: &Path) {
    command
        .env("GIT_EDITOR", sandbox_root.join("host-editor"))
        .env("VISUAL", sandbox_root.join("host-visual"))
        .env("EDITOR", sandbox_root.join("host-default-editor"))
        .env(
            "GIT_SEQUENCE_EDITOR",
            sandbox_root.join("host-sequence-editor"),
        )
        .env("GIT_DIR", sandbox_root.join("host-git-dir"))
        .env("GIT_COMMON_DIR", sandbox_root.join("host-git-common-dir"))
        .env("GIT_WORK_TREE", sandbox_root.join("host-work-tree"))
        .env(
            "GIT_CONFIG_SYSTEM",
            sandbox_root.join("host-system.gitconfig"),
        )
        .env(
            "GIT_CONFIG_GLOBAL",
            sandbox_root.join("host-global.gitconfig"),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "2")
        .env("GIT_CONFIG_KEY_0", "core.editor")
        .env("GIT_CONFIG_VALUE_0", "host-config-editor")
        .env("GIT_CONFIG_KEY_1", "core.worktree")
        .env(
            "GIT_CONFIG_VALUE_1",
            sandbox_root.join("host-config-work-tree"),
        );
}

fn create_native_fixture(sandbox: &Sandbox, env: &IsolatedEnv) -> Result<PathBuf> {
    let repo_path = sandbox.fixtures.join("native-repo");
    fs::create_dir_all(&repo_path)?;
    command_checked(env, "git", &["init", "-q", "-b", "main"], &repo_path)?;
    command_checked(
        env,
        "git",
        &["config", "user.name", "Viewer E2E"],
        &repo_path,
    )?;
    command_checked(
        env,
        "git",
        &["config", "user.email", "viewer-e2e@example.invalid"],
        &repo_path,
    )?;
    fs::write(repo_path.join("work.txt"), "base\n")?;
    command_checked(env, "git", &["add", "work.txt"], &repo_path)?;
    command_checked(env, "git", &["commit", "-q", "-m", "base"], &repo_path)?;
    fs::write(repo_path.join("work.txt"), "base\nwarm forwarding\n")?;
    Ok(repo_path)
}

fn release_binary(name: &str) -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("resolve repository root for release binary")?
        .join("target")
        .join("release")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
}

fn available_display(range: std::ops::Range<u16>) -> Option<u16> {
    range.into_iter().find(|number| {
        !Path::new(&format!("/tmp/.X{number}-lock")).exists()
            && !Path::new(&format!("/tmp/.X11-unix/X{number}")).exists()
    })
}

fn find_window(env: &IsolatedEnv, pattern: &str) -> Result<String> {
    let result = output(
        env,
        "xdotool",
        &["search", "--onlyvisible", "--name", pattern],
        Path::new("."),
    )?;
    if !result.status.success() {
        bail!("window `{pattern}` not found");
    }
    String::from_utf8_lossy(&result.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .context("window search returned no id")
}

fn output(env: &IsolatedEnv, program: &str, args: &[&str], cwd: &Path) -> Result<Output> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    env.apply(&mut command);
    command
        .output()
        .with_context(|| format!("run {program} {}", args.join(" ")))
}

fn command_success(env: &IsolatedEnv, program: &str, args: &[&str], cwd: &Path) -> bool {
    output(env, program, args, cwd).is_ok_and(|result| result.status.success())
}

fn command_checked(env: &IsolatedEnv, program: &str, args: &[&str], cwd: &Path) -> Result<()> {
    let result = output(env, program, args, cwd)?;
    if result.status.success() {
        return Ok(());
    }
    bail!(
        "{program} {} failed (exit {}): {}{}",
        args.join(" "),
        result.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    )
}

fn retry(label: &str, timeout: Duration, mut ready: impl FnMut() -> bool) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if ready() {
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
    bail!("timed out waiting for {label}")
}

fn retry_value<T>(
    label: &str,
    timeout: Duration,
    mut value: impl FnMut() -> Option<T>,
) -> Result<T> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(value) = value() {
            return Ok(value);
        }
        thread::sleep(POLL_INTERVAL);
    }
    bail!("timed out waiting for {label}")
}

fn preserve_logs(sandbox: &Sandbox, directory_name: &str) -> Result<()> {
    let destination = Path::new(".artifacts/logs").join(directory_name);
    if destination.exists() {
        fs::remove_dir_all(&destination).context("replace previous desktop E2E logs")?;
    }
    fs::create_dir_all(&destination)?;
    for entry in fs::read_dir(&sandbox.logs)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::{OsStr, OsString},
        fs,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    use super::{
        HostCargoEnvironment, IsolatedEnv, cargo_runner_config, clear_evidence_outcomes,
        runtime_command,
    };

    #[test]
    fn plain_run_clears_failures_and_preserves_success_evidence() {
        let evidence_root = tempfile::tempdir().unwrap();
        let success = evidence_root.path().join("success/thirtyfour/viewer.png");
        let failure = evidence_root.path().join("fail/thirtyfour/viewer.png");
        for screenshot in [&success, &failure] {
            fs::create_dir_all(screenshot.parent().unwrap()).unwrap();
            fs::write(screenshot, b"stale").unwrap();
        }

        clear_evidence_outcomes(evidence_root.path(), false).unwrap();

        assert!(success.is_file());
        assert!(!failure.exists());
    }

    #[test]
    fn enabled_evidence_root_removes_stale_outcomes() {
        let evidence_root = tempfile::tempdir().unwrap();
        for outcome in ["success", "fail"] {
            let screenshot = evidence_root.path().join(outcome).join("stale.png");
            fs::create_dir_all(screenshot.parent().unwrap()).unwrap();
            fs::write(screenshot, b"stale").unwrap();
        }

        clear_evidence_outcomes(evidence_root.path(), true).unwrap();

        assert!(!evidence_root.path().join("success").exists());
        assert!(!evidence_root.path().join("fail").exists());
    }

    #[test]
    fn isolated_environment_rejects_host_git_overrides() {
        let repository = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(repository.path())
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["config", "core.editor", "repository-editor"])
                .current_dir(repository.path())
                .status()
                .unwrap()
                .success()
        );
        let pairs = super::HOST_ENVIRONMENT_VARIABLE_NAMES
            .iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (OsString::from(name), value)))
            .collect();
        let environment = IsolatedEnv { pairs };
        let host_environment = HostCargoEnvironment::from_environment_with_home_variable(
            |name| match name {
                "HOME" => Some(OsString::from("/host/home")),
                "SSH_AUTH_SOCK" => Some(OsString::from("/host/agent.sock")),
                _ => None,
            },
            "HOME",
        )
        .unwrap();
        let hostile_git_directory = repository.path().join("host-git-dir");
        fs::create_dir(&hostile_git_directory).unwrap();
        let mut command = Command::new("git");
        command
            .args(["var", "GIT_EDITOR"])
            .current_dir(repository.path())
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .env("GIT_EDITOR", "host-editor")
            .env("GIT_DIR", &hostile_git_directory)
            .env("GIT_WORK_TREE", repository.path().join("host-work-tree"))
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.editor")
            .env("GIT_CONFIG_VALUE_0", "injected-editor");
        environment.apply_cargo(&mut command, &host_environment);

        let output = command.output().unwrap();

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "repository-editor"
        );
    }

    #[test]
    fn cargo_environment_uses_unix_home_for_default_build_paths_and_agent() {
        let host_environment = HostCargoEnvironment::from_environment_with_home_variable(
            |name| match name {
                "HOME" => Some(OsString::from("/host/home")),
                "SSH_AUTH_SOCK" => Some(OsString::from("/host/agent.sock")),
                _ => None,
            },
            "HOME",
        )
        .unwrap();
        let environment = IsolatedEnv {
            pairs: vec![
                (OsString::from("HOME"), OsString::from("/sandbox/home")),
                (
                    OsString::from("XDG_CACHE_HOME"),
                    OsString::from("/sandbox/cache"),
                ),
            ],
        };
        let mut command = Command::new("cargo");

        environment.apply_cargo(&mut command, &host_environment);

        let values = command.get_envs().collect::<Vec<_>>();
        assert_eq!(
            command_environment_value(&values, "HOME"),
            Some(OsStr::new("/sandbox/home"))
        );
        assert_eq!(
            command_environment_value(&values, "XDG_CACHE_HOME"),
            Some(OsStr::new("/sandbox/cache"))
        );
        assert_eq!(
            command_environment_value(&values, "CARGO_HOME"),
            Some(OsStr::new("/host/home/.cargo"))
        );
        assert_eq!(
            command_environment_value(&values, "RUSTUP_HOME"),
            Some(OsStr::new("/host/home/.rustup"))
        );
        assert_eq!(
            command_environment_value(&values, "SSH_AUTH_SOCK"),
            Some(OsStr::new("/host/agent.sock"))
        );
    }

    #[test]
    fn cargo_environment_uses_explicit_host_build_paths() {
        let host_environment = HostCargoEnvironment::from_environment(|name| match name {
            "HOME" => Some(OsString::from("/host/home")),
            "CARGO_HOME" => Some(OsString::from("/host/cargo")),
            "RUSTUP_HOME" => Some(OsString::from("/host/rustup")),
            _ => None,
        })
        .unwrap();
        let environment = IsolatedEnv { pairs: Vec::new() };
        let mut command = Command::new("cargo");

        environment.apply_cargo(&mut command, &host_environment);

        let values = command.get_envs().collect::<Vec<_>>();
        assert_eq!(
            command_environment_value(&values, "CARGO_HOME"),
            Some(OsStr::new("/host/cargo"))
        );
        assert_eq!(
            command_environment_value(&values, "RUSTUP_HOME"),
            Some(OsStr::new("/host/rustup"))
        );
        assert_eq!(command_environment_value(&values, "SSH_AUTH_SOCK"), None);
    }

    #[test]
    fn cargo_environment_uses_windows_user_profile_for_default_build_paths() {
        let host_environment = HostCargoEnvironment::from_environment_with_home_variable(
            |name| match name {
                "HOME" => Some(OsString::from("/git-bash/home")),
                "USERPROFILE" => Some(OsString::from(r"C:\Users\developer")),
                _ => None,
            },
            "USERPROFILE",
        )
        .unwrap();
        let environment = IsolatedEnv { pairs: Vec::new() };
        let mut command = Command::new("cargo");

        environment.apply_cargo(&mut command, &host_environment);

        let values = command.get_envs().collect::<Vec<_>>();
        assert_eq!(
            command_environment_value(&values, "CARGO_HOME"),
            Some(
                PathBuf::from(r"C:\Users\developer")
                    .join(".cargo")
                    .as_os_str()
            )
        );
        assert_eq!(
            command_environment_value(&values, "RUSTUP_HOME"),
            Some(
                PathBuf::from(r"C:\Users\developer")
                    .join(".rustup")
                    .as_os_str()
            )
        );
    }

    #[test]
    fn cargo_runner_uses_xtask_as_the_runtime_boundary() {
        let config = cargo_runner_config(Path::new("/repo/target/debug/xtask"));

        assert_eq!(
            config,
            "[target.'cfg(all())']\nrunner = [\"/repo/target/debug/xtask\", \"e2e-runtime-worker\"]\n"
        );
    }

    #[test]
    fn runtime_boundary_removes_host_build_environment() {
        let command = runtime_command(
            Path::new("/repo/target/debug/deps/viewer"),
            &[OsString::from("--test-threads"), OsString::from("1")],
            None,
        );
        let values = command.get_envs().collect::<Vec<_>>();

        assert_eq!(
            command.get_program(),
            OsStr::new("/repo/target/debug/deps/viewer")
        );
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [OsStr::new("--test-threads"), OsStr::new("1")]
        );
        for name in ["CARGO_HOME", "RUSTUP_HOME", "SSH_AUTH_SOCK"] {
            assert_eq!(command_environment_value(&values, name), None);
            assert!(
                values
                    .iter()
                    .any(|(current, value)| *current == OsStr::new(name) && value.is_none())
            );
        }
        assert_eq!(
            command_environment_value(&values, "GTL_E2E_RUNTIME_ISOLATED"),
            Some(OsStr::new("1"))
        );
    }

    fn command_environment_value<'a>(
        values: &'a [(&'a OsStr, Option<&'a OsStr>)],
        name: &str,
    ) -> Option<&'a OsStr> {
        values
            .iter()
            .find(|(current, _)| *current == OsStr::new(name))
            .and_then(|(_, value)| value.as_deref())
    }
}
