//! Hermetic desktop viewer E2E. Linux uses an isolated Xvfb/Openbox/stalonetray/D-Bus session
//! for native lifecycle assertions, then runs the `WebDriver` DOM suite in the same sandbox.

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

use anyhow::{Context, Result, bail};
use command_group::{CommandGroup, GroupChild};

use super::{build, status_notifier::StatusNotifierWatcher};
use crate::{
    cli::BuildTarget,
    process::{self, Status},
    verb::Verb,
};

mod playwright;
mod stable_runner;

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const DOM_DAEMON_STORE_MAX: usize = 64;
const WINDOW_TITLE_PATTERN: &str = "^git-tools diff viewer$";
const EVIDENCE_OUTPUT_PATH_ENVIRONMENT_VARIABLE: &str = "GTL_E2E_EVIDENCE_OUTPUT_PATH";
const EVIDENCES_OUTPUT_PATH_ENVIRONMENT_VARIABLE: &str = "TEST_EVIDENCES_OUTPUT_PATH";
const EVIDENCE_OUTPUT_PATH_DEFAULT: &str = ".artifacts/e2e";
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
    editor_recorder: PathBuf,
    editor_record: PathBuf,
    editor_release: PathBuf,
    editor_exit: PathBuf,
    cli_binary: PathBuf,
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
            editor_recorder: root
                .join("fixtures")
                .join(format!("code{}", std::env::consts::EXE_SUFFIX)),
            editor_record: root.join("logs/editor-record.json"),
            editor_release: root.join("logs/editor-release"),
            editor_exit: root.join("logs/editor-exit"),
            cli_binary: release_binary("git-tools")?,
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
        if std::env::consts::OS == "linux" {
            let status = Command::new("chmod")
                .args(["700", sandbox.runtime.to_string_lossy().as_ref()])
                .status()
                .context("set XDG_RUNTIME_DIR permissions")?;
            if !status.success() {
                bail!("chmod 700 for XDG_RUNTIME_DIR failed");
            }
        }
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
            ("GTL_E2E_DATA_ROOT", data_root.as_os_str()),
            ("GTL_E2E_FIXTURE_ROOT", self.fixtures.as_os_str()),
            ("GTL_E2E_EDITOR_RECORDER", self.editor_recorder.as_os_str()),
            ("GTL_E2E_EDITOR_RECORD", self.editor_record.as_os_str()),
            ("GTL_E2E_EDITOR_RELEASE", self.editor_release.as_os_str()),
            ("GTL_E2E_EDITOR_EXIT", self.editor_exit.as_os_str()),
            ("GTL_E2E_CLI_BINARY", self.cli_binary.as_os_str()),
            ("GTL_E2E_VIEWER_BINARY", self.viewer_binary.as_os_str()),
            (
                EVIDENCE_OUTPUT_PATH_ENVIRONMENT_VARIABLE,
                self.evidence_root.as_os_str(),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value.to_os_string()))
        .collect::<Vec<_>>();
        for name in HOST_ENVIRONMENT_VARIABLE_NAMES {
            if let Some(value) = std::env::var_os(name) {
                pairs.push((OsString::from(name), value));
            }
        }
        if self.success_evidence_requested {
            pairs.push((
                OsString::from(EVIDENCES_OUTPUT_PATH_ENVIRONMENT_VARIABLE),
                self.evidence_root.as_os_str().to_os_string(),
            ));
        }
        IsolatedEnv { pairs }
    }
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

    fn is_running(&mut self) -> Result<bool> {
        Ok(self
            .child
            .as_mut()
            .context("managed child already consumed")?
            .try_wait()?
            .is_none())
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

#[derive(Debug)]
struct DaemonCleanupGuard {
    cli: PathBuf,
    cwd: PathBuf,
    environments: Vec<(String, IsolatedEnv)>,
    dom_data: PathBuf,
    dom_environment: IsolatedEnv,
    stopped: bool,
}

impl DaemonCleanupGuard {
    fn new(sandbox: &Sandbox) -> Result<Self> {
        let dom_environment = sandbox.environment(&sandbox.dom_data);
        Ok(Self {
            cli: release_binary("git-tools")?,
            cwd: sandbox.root.clone(),
            environments: vec![
                ("native".into(), sandbox.environment(&sandbox.native_data)),
                ("dom".into(), dom_environment.clone()),
                ("browser".into(), sandbox.environment(&sandbox.browser_data)),
            ],
            dom_data: sandbox.dom_data.clone(),
            dom_environment,
            stopped: false,
        })
    }

    fn stop(&mut self) -> Result<()> {
        let mut failures = Vec::new();
        let mut environments = self.environments.clone();
        match daemon_data_roots(&self.dom_data) {
            Ok(data_roots) => {
                for data_root in data_roots {
                    let mut environment = self.dom_environment.clone();
                    environment.set("GIT_TOOLS_DATA_DIR", &data_root);
                    environment.set("GTL_E2E_DATA_ROOT", &data_root);
                    let name: String = data_root
                        .file_name()
                        .map_or_else(|| "dom/spec".into(), |name| name.to_string_lossy().into());
                    environments.push((format!("dom/{name}"), environment));
                }
            }
            Err(error) => failures.push(format!("DOM store discovery: {error:#}")),
        }
        for (name, environment) in &environments {
            if let Err(error) = command_checked(
                environment,
                self.cli.to_string_lossy().as_ref(),
                &["daemon", "stop"],
                &self.cwd,
            ) {
                failures.push(format!("{name} store: {error:#}"));
            }
        }
        if !failures.is_empty() {
            bail!(
                "failed to stop {} isolated daemon(s): {}",
                failures.len(),
                failures.join("; ")
            );
        }
        self.stopped = true;
        Ok(())
    }
}

fn daemon_data_roots(dom_data: &Path) -> Result<Vec<PathBuf>> {
    let mut roots = fs::read_dir(dom_data)
        .with_context(|| format!("read DOM data root {}", dom_data.display()))?
        .filter_map(|entry| match entry {
            Ok(entry) => match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => Some(Ok(entry.path())),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            },
            Err(error) => Some(Err(error)),
        })
        .collect::<std::io::Result<Vec<_>>>()?;
    roots.sort();
    if roots.len() > DOM_DAEMON_STORE_MAX {
        bail!(
            "DOM daemon stores exceed maximum {DOM_DAEMON_STORE_MAX}: found {}",
            roots.len()
        );
    }
    Ok(roots)
}

impl Drop for DaemonCleanupGuard {
    fn drop(&mut self) {
        if !self.stopped
            && let Err(error) = self.stop()
        {
            eprintln!("desktop-e2e: failed to stop an isolated daemon: {error:#}");
        }
    }
}

/// Build and run the platform desktop E2E workflow.
pub fn run() -> Result<()> {
    let result = workflow();
    match &result {
        Ok(()) => process::result(Verb::DESKTOP_E2E, Status::Pass),
        Err(_) => process::result_fail_step(Verb::DESKTOP_E2E, "workflow"),
    }
    result
}

pub(crate) fn run_worker(_verbose: bool) -> Result<()> {
    run()
}

pub(crate) fn run_runtime(executable: &Path, arguments: &[OsString]) -> Result<()> {
    let mut command = runtime_command(executable, arguments);
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

fn runtime_command(executable: &Path, arguments: &[OsString]) -> Command {
    let mut command = Command::new(executable);
    command.args(arguments).env("GTL_E2E_RUNTIME_ISOLATED", "1");
    for name in ["CARGO_HOME", "RUSTUP_HOME", "SSH_AUTH_SOCK"] {
        command.env_remove(name);
    }
    command
}

fn cargo_runner_config(executable: &Path) -> String {
    format!(
        "[target.'cfg(all())']\nrunner = [{:?}, \"e2e-runtime-worker\"]\n",
        executable.to_string_lossy()
    )
}

fn workflow() -> Result<()> {
    require_tool("tauri-driver", "run `mise install cargo:tauri-driver`")?;
    build::run(BuildTarget::Cli)?;
    build::run(BuildTarget::Viewer)?;
    process::run(
        "viewer-e2e-editor-recorder-build",
        "cargo",
        &[
            "build",
            "--release",
            "-p",
            "gtl-e2e",
            "--bin",
            "editor-recorder",
        ],
    )?;

    let sandbox = Sandbox::create()?;
    clear_evidence_outcomes(&sandbox.evidence_root, sandbox.success_evidence_requested)?;
    fs::copy(release_binary("editor-recorder")?, &sandbox.editor_recorder)
        .context("copy editor recorder fixture")?;
    let mut daemon_cleanup = DaemonCleanupGuard::new(&sandbox)?;
    let result = match std::env::consts::OS {
        "linux" => run_linux(&sandbox),
        "windows" => run_browser_phases(&sandbox, &sandbox.environment(&sandbox.dom_data)),
        unsupported => bail!(
            "hermetic desktop E2E is not configured for {unsupported}; Linux and Windows are supported"
        ),
    };
    let editor_cleanup_result = release_editor_recorder(&sandbox);
    let cleanup_result = daemon_cleanup.stop();
    let logs_result = preserve_logs(&sandbox);
    result?;
    editor_cleanup_result?;
    cleanup_result?;
    logs_result
}

fn run_linux(sandbox: &Sandbox) -> Result<()> {
    preflight_linux()?;
    let display = available_display(90..190)
        .context("no free isolated X display number in the 90..190 range")?;
    let display_value = format!(":{display}");
    fs::write(sandbox.root.join("display"), &display_value)?;
    let mut env = sandbox.environment(&sandbox.native_data);
    env.set("DISPLAY", &display_value);
    env.set("NO_AT_BRIDGE", "1");

    let _xvfb = ManagedChild::spawn(
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
        &env,
        &sandbox.root,
        &sandbox.logs.join("xvfb.log"),
    )?;
    retry("Xvfb readiness", READY_TIMEOUT, || {
        command_success(
            &env,
            "xdpyinfo",
            &["-display", &display_value],
            &sandbox.root,
        )
    })?;

    let (_dbus, tray_watcher) = start_private_dbus(sandbox, &mut env)?;

    let _openbox = ManagedChild::spawn(
        "Openbox",
        "openbox",
        &["--sm-disable"],
        &env,
        &sandbox.root,
        &sandbox.logs.join("openbox.log"),
    )?;
    retry("Openbox readiness", READY_TIMEOUT, || {
        output(
            &env,
            "xprop",
            &["-root", "_NET_SUPPORTING_WM_CHECK"],
            &sandbox.root,
        )
        .is_ok_and(|result| result.status.success())
    })?;

    let _tray = ManagedChild::spawn(
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
        &env,
        &sandbox.root,
        &sandbox.logs.join("stalonetray.log"),
    )?;
    retry("stalonetray readiness", READY_TIMEOUT, || {
        find_window(&env, "^stalonetray$").is_ok()
    })?;

    let result = run_native_phase(sandbox, &env, &tray_watcher).and_then(|()| {
        let mut dom_env = env.clone();
        dom_env.set("GIT_TOOLS_DATA_DIR", sandbox.dom_data.as_os_str());
        dom_env.set("GTL_E2E_DATA_ROOT", sandbox.dom_data.as_os_str());
        run_browser_phases(sandbox, &dom_env)
    });
    if result.is_err() {
        capture_diagnostics(sandbox, &env);
    }
    result
}

fn start_private_dbus(
    sandbox: &Sandbox,
    env: &mut IsolatedEnv,
) -> Result<(ManagedChild, StatusNotifierWatcher)> {
    let address_file = sandbox.root.join("dbus-address");
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
        &sandbox.logs.join("dbus.log"),
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

fn preflight_linux() -> Result<()> {
    for (tool, hint) in [
        ("Xvfb", "run `mise bootstrap packages apply apt:xvfb`"),
        ("openbox", "run `mise bootstrap packages apply apt:openbox`"),
        (
            "stalonetray",
            "run `mise bootstrap packages apply apt:stalonetray`",
        ),
        (
            "dbus-run-session",
            "run `mise bootstrap packages apply apt:dbus-daemon`",
        ),
        ("xdotool", "run `mise bootstrap packages apply apt:xdotool`"),
        (
            "xwininfo",
            "run `mise bootstrap packages apply apt:x11-utils`",
        ),
        (
            "xdpyinfo",
            "run `mise bootstrap packages apply apt:x11-utils`",
        ),
        (
            "WebKitWebDriver",
            "run `mise bootstrap packages apply apt:webkit2gtk-driver`",
        ),
    ] {
        require_tool(tool, hint)?;
    }
    Ok(())
}

fn run_native_phase(
    sandbox: &Sandbox,
    env: &IsolatedEnv,
    tray_watcher: &StatusNotifierWatcher,
) -> Result<()> {
    let viewer = release_binary("gtl-viewer")?;
    let cli = release_binary("git-tools")?;
    let mut viewer = ManagedChild::spawn(
        "native viewer",
        viewer.to_string_lossy().as_ref(),
        &[],
        env,
        &sandbox.root,
        &sandbox.logs.join("native-viewer.log"),
    )?;
    let window = retry_value("cold viewer map", READY_TIMEOUT, || {
        find_window(env, WINDOW_TITLE_PATTERN).ok()
    })?;
    assert_viewable(env, &window, &sandbox.root)?;
    retry("viewer tray registration", READY_TIMEOUT, || {
        tray_watcher.has_registered_item()
    })?;

    command_checked(
        env,
        "xdotool",
        &["windowactivate", "--sync", &window, "key", "alt+F4"],
        &sandbox.root,
    )?;
    retry("close-to-hide", READY_TIMEOUT, || {
        find_window(env, WINDOW_TITLE_PATTERN).is_err()
    })?;
    if !viewer.is_running()? {
        bail!("close-to-hide terminated the original viewer process");
    }

    let repo = create_native_fixture(sandbox, env)?;
    command_checked(
        env,
        cli.to_string_lossy().as_ref(),
        &["diff", "--name", "warm forwarding"],
        &repo,
    )?;
    let forwarded_window = retry_value("warm forwarding remap", READY_TIMEOUT, || {
        find_window(env, WINDOW_TITLE_PATTERN).ok()
    })?;
    if forwarded_window != window {
        bail!("warm forwarding mapped a replacement viewer window");
    }
    retry("warm forwarding focus", READY_TIMEOUT, || {
        output(env, "xdotool", &["getactivewindow"], &sandbox.root)
            .ok()
            .filter(|result| result.status.success())
            .is_some_and(|result| {
                String::from_utf8_lossy(&result.stdout).trim() == forwarded_window
            })
    })?;
    if !viewer.is_running()? {
        bail!("warm forwarding replaced the original viewer process");
    }
    Ok(())
}

fn run_dom_phase(
    sandbox: &Sandbox,
    env: &IsolatedEnv,
    host_environment: &HostCargoEnvironment,
) -> Result<()> {
    let log = sandbox.logs.join("thirtyfour.log");
    let arguments = [
        "test",
        "-p",
        "gtl-desktop-e2e",
        "--features",
        "e2e",
        "--test",
        "viewer",
        "--",
        "--test-threads",
        "1",
    ];
    let mut command = Command::new("cargo");
    command
        .arg("--config")
        .arg(&sandbox.cargo_runner_config)
        .args(arguments)
        .current_dir(".");
    seed_hostile_git_environment(&mut command, &sandbox.root);
    env.apply_cargo(&mut command, host_environment);
    let result = command.output().context("run Thirtyfour viewer E2E")?;
    let mut bytes = result.stdout;
    bytes.extend_from_slice(&result.stderr);
    fs::write(&log, &bytes)?;
    if !result.status.success() {
        std::io::stderr().write_all(&bytes)?;
        bail!("viewer Thirtyfour DOM phase failed");
    }
    Ok(())
}

fn run_browser_phases(sandbox: &Sandbox, env: &IsolatedEnv) -> Result<()> {
    let host_environment = HostCargoEnvironment::capture()?;
    run_dom_phase(sandbox, env, &host_environment)?;
    playwright::run(
        sandbox,
        &sandbox.environment(&sandbox.browser_data),
        &host_environment,
    )
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

fn release_editor_recorder(sandbox: &Sandbox) -> Result<()> {
    fs::write(&sandbox.editor_release, b"release").context("release editor recorder")?;
    if !sandbox.editor_record.is_file() {
        return Ok(());
    }
    retry("editor recorder exit", READY_TIMEOUT, || {
        sandbox.editor_exit.is_file()
    })
}

fn create_native_fixture(sandbox: &Sandbox, env: &IsolatedEnv) -> Result<PathBuf> {
    let repo = sandbox.fixtures.join("native-repo");
    fs::create_dir_all(&repo)?;
    command_checked(env, "git", &["init", "-q", "-b", "main"], &repo)?;
    command_checked(env, "git", &["config", "user.name", "Viewer E2E"], &repo)?;
    command_checked(
        env,
        "git",
        &["config", "user.email", "viewer-e2e@example.invalid"],
        &repo,
    )?;
    fs::write(repo.join("work.txt"), "base\n")?;
    command_checked(env, "git", &["add", "work.txt"], &repo)?;
    command_checked(env, "git", &["commit", "-q", "-m", "base"], &repo)?;
    fs::write(repo.join("work.txt"), "base\nwarm forwarding\n")?;
    Ok(repo)
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

fn window_is_viewable(env: &IsolatedEnv, window: &str, cwd: &Path) -> bool {
    output(env, "xwininfo", &["-id", window], cwd)
        .ok()
        .filter(|result| result.status.success())
        .is_some_and(|result| {
            String::from_utf8_lossy(&result.stdout).contains("Map State: IsViewable")
        })
}

fn assert_viewable(env: &IsolatedEnv, window: &str, cwd: &Path) -> Result<()> {
    if !window_is_viewable(env, window, cwd) {
        bail!("viewer window {window} is not mapped");
    }
    Ok(())
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

fn capture_diagnostics(sandbox: &Sandbox, env: &IsolatedEnv) {
    for (name, program, args) in [
        ("xprop-root.log", "xprop", vec!["-root"]),
        ("xwininfo-root.log", "xwininfo", vec!["-root", "-tree"]),
    ] {
        if let Ok(result) = output(env, program, &args, &sandbox.root) {
            let mut bytes = result.stdout;
            bytes.extend_from_slice(&result.stderr);
            let _ = fs::write(sandbox.logs.join(name), bytes);
        }
    }
}

fn preserve_logs(sandbox: &Sandbox) -> Result<()> {
    let destination = Path::new(".artifacts/logs/desktop-e2e");
    if destination.exists() {
        fs::remove_dir_all(destination).context("replace previous desktop E2E logs")?;
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(&sandbox.logs)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn require_tool(name: &str, hint: &str) -> Result<()> {
    which::which(name).with_context(|| format!("required tool `{name}` is missing; {hint}"))?;
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
        DOM_DAEMON_STORE_MAX, HostCargoEnvironment, IsolatedEnv, cargo_runner_config,
        clear_evidence_outcomes, daemon_data_roots, runtime_command,
    };

    #[test]
    fn daemon_cleanup_discovers_bounded_direct_dom_stores() {
        let dom_root = tempfile::tempdir().expect("temporary DOM data root");
        let one_shot = dom_root.path().join("viewer-one-shot-lifecycle");
        let live = dom_root.path().join("viewer-live-lifecycle");
        fs::create_dir_all(one_shot.join("nested")).expect("create one-shot data root");
        fs::create_dir(&live).expect("create live data root");
        fs::write(dom_root.path().join("state.db"), b"ignored").expect("write root file");

        let roots = daemon_data_roots(dom_root.path()).expect("discover daemon data roots");

        assert_eq!(roots, vec![live, one_shot]);
    }

    #[test]
    fn daemon_cleanup_rejects_unbounded_dom_stores() {
        let dom_root = tempfile::tempdir().expect("temporary DOM data root");
        for index in 0..=DOM_DAEMON_STORE_MAX {
            fs::create_dir(dom_root.path().join(format!("suite-{index:02}")))
                .expect("create DOM data root");
        }

        let error = daemon_data_roots(dom_root.path()).unwrap_err();

        assert!(error.to_string().contains("maximum"));
    }

    #[test]
    fn plain_run_clears_failures_and_preserves_success_evidence() {
        let evidence_root = tempfile::tempdir().expect("temporary evidence root");
        let success = evidence_root.path().join("success/thirtyfour/viewer.png");
        let failure = evidence_root.path().join("fail/thirtyfour/viewer.png");
        for screenshot in [&success, &failure] {
            fs::create_dir_all(screenshot.parent().expect("screenshot parent"))
                .expect("create stale evidence parent");
            fs::write(screenshot, b"stale").expect("write stale evidence");
        }

        clear_evidence_outcomes(evidence_root.path(), false).expect("clear failure evidence");

        assert!(success.is_file());
        assert!(!failure.exists());
    }

    #[test]
    fn enabled_evidence_root_removes_stale_outcomes() {
        let evidence_root = tempfile::tempdir().expect("temporary evidence root");
        for outcome in ["success", "fail"] {
            let screenshot = evidence_root.path().join(outcome).join("stale.png");
            fs::create_dir_all(screenshot.parent().expect("screenshot parent"))
                .expect("create stale evidence parent");
            fs::write(screenshot, b"stale").expect("write stale evidence");
        }

        clear_evidence_outcomes(evidence_root.path(), true).expect("clear stale evidence");

        assert!(!evidence_root.path().join("success").exists());
        assert!(!evidence_root.path().join("fail").exists());
    }

    #[test]
    fn isolated_environment_rejects_host_git_overrides() {
        let repository = tempfile::tempdir().expect("temporary repository");
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(repository.path())
                .status()
                .expect("initialize repository")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["config", "core.editor", "repository-editor"])
                .current_dir(repository.path())
                .status()
                .expect("configure repository editor")
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
        .expect("derive host build environment");
        let hostile_git_directory = repository.path().join("host-git-dir");
        fs::create_dir(&hostile_git_directory).expect("create hostile Git directory");
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

        let output = command.output().expect("query configured editor");

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)
                .expect("UTF-8 editor")
                .trim(),
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
        .expect("derive host build environment");
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
        .expect("capture explicit host build environment");
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
        .expect("derive Windows host build environment");
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
