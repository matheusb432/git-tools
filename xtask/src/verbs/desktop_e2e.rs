//! Hermetic desktop viewer E2E. Linux uses an isolated Xvfb/Openbox/stalonetray/D-Bus session
//! for native lifecycle assertions, then runs the `WebDriver` DOM suite in the same sandbox.

use std::{
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

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const WINDOW_TITLE_PATTERN: &str = "^git-tools diff viewer$";

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
}

impl Sandbox {
    fn create() -> Result<Self> {
        let guard = tempfile::Builder::new()
            .prefix("gtl-viewer-e2e-")
            .tempdir()
            .context("create viewer E2E sandbox")?;
        let root = guard.path().to_path_buf();
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
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value.to_os_string()))
        .collect::<Vec<_>>();
        if let Some(path) = std::env::var_os("PATH") {
            pairs.push((OsString::from("PATH"), path));
        }
        IsolatedEnv { pairs }
    }
}

#[derive(Debug, Clone)]
struct IsolatedEnv {
    pairs: Vec<(OsString, OsString)>,
}

impl IsolatedEnv {
    fn set(&mut self, name: &str, value: impl AsRef<OsStr>) {
        let name = OsString::from(name);
        self.pairs.retain(|(current, _)| current != &name);
        self.pairs.push((name, value.as_ref().to_os_string()));
    }

    fn apply(&self, command: &mut Command) {
        command
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .envs(self.pairs.iter().cloned());
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
    environments: Vec<(&'static str, IsolatedEnv)>,
    stopped: bool,
}

impl DaemonCleanupGuard {
    fn new(sandbox: &Sandbox) -> Result<Self> {
        Ok(Self {
            cli: release_binary("git-tools")?,
            cwd: sandbox.root.clone(),
            environments: vec![
                ("native", sandbox.environment(&sandbox.native_data)),
                ("dom", sandbox.environment(&sandbox.dom_data)),
            ],
            stopped: false,
        })
    }

    fn stop(&mut self) -> Result<()> {
        let mut failures = Vec::new();
        for (name, environment) in &self.environments {
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

fn workflow() -> Result<()> {
    require_tool("deno", "install Deno through sample_project provisioning")?;
    require_tool(
        "tauri-driver",
        "install tauri-driver through sample_project provisioning",
    )?;
    process::run_in(
        "viewer-e2e-dependencies",
        "e2e/viewer",
        "deno",
        &["install", "--frozen"],
    )?;
    build::run(BuildTarget::Cli)?;
    build::run(BuildTarget::Viewer)?;

    let sandbox = Sandbox::create()?;
    let mut daemon_cleanup = DaemonCleanupGuard::new(&sandbox)?;
    let result = match std::env::consts::OS {
        "linux" => run_linux(&sandbox),
        "windows" => run_dom_phase(&sandbox, &sandbox.environment(&sandbox.dom_data)),
        unsupported => bail!(
            "hermetic desktop E2E is not configured for {unsupported}; Linux and Windows are supported"
        ),
    };
    let cleanup_result = daemon_cleanup.stop();
    let logs_result = preserve_logs(&sandbox);
    result?;
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
        run_dom_phase(sandbox, &dom_env)
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
        ("Xvfb", "install xvfb through sample_project provisioning"),
        ("openbox", "install openbox through sample_project provisioning"),
        (
            "stalonetray",
            "install stalonetray through sample_project provisioning",
        ),
        ("dbus-run-session", "install dbus through sample_project provisioning"),
        ("xdotool", "install xdotool through sample_project provisioning"),
        ("xwininfo", "install x11-utils through sample_project provisioning"),
        ("xdpyinfo", "install x11-utils through sample_project provisioning"),
        (
            "WebKitWebDriver",
            "install webkit2gtk-driver through sample_project provisioning",
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

fn run_dom_phase(sandbox: &Sandbox, env: &IsolatedEnv) -> Result<()> {
    let log = sandbox.logs.join("webdriver.log");
    let result = output(
        env,
        "deno",
        &["task", "--frozen", "--cwd", "e2e/viewer", "test:e2e"],
        Path::new("."),
    )?;
    let mut bytes = result.stdout;
    bytes.extend_from_slice(&result.stderr);
    fs::write(&log, &bytes)?;
    if !result.status.success() {
        std::io::stderr().write_all(&bytes)?;
        bail!("viewer WebDriver DOM phase failed");
    }
    Ok(())
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
