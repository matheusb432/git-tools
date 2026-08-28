use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_POLL_DELAY: Duration = Duration::from_millis(20);
const DIAGNOSTIC_BYTES_MAX: usize = 16 * 1_024;

pub struct ReleaseServerConfig<'config> {
    pub server_binary: &'config Path,
    pub settings: &'config str,
    pub profiler: Option<MassifProfiler<'config>>,
}

pub struct MassifProfiler<'config> {
    pub valgrind_binary: &'config Path,
    pub output_path: &'config Path,
}

pub struct ReleaseServerProcess {
    child: Child,
    data_root: PathBuf,
    stderr_path: PathBuf,
    stopped: bool,
}

impl ReleaseServerProcess {
    pub fn start(config: ReleaseServerConfig<'_>, root: &Path) -> Result<Self> {
        let bin = root.join("bin");
        let data_root = root.join("data");
        let logs = root.join("logs");
        let settings = root.join("config.toml");
        fs::create_dir_all(&bin).context("create isolated server binary directory")?;
        fs::create_dir_all(&data_root).context("create isolated server data root")?;
        fs::create_dir_all(&logs).context("create isolated server log directory")?;
        fs::write(&settings, config.settings).context("write isolated benchmark settings")?;
        let server = bin.join("gtl-server");
        let viewer = bin.join("gtl-viewer");
        fs::copy(config.server_binary, &server).with_context(|| {
            format!(
                "copy release server {} to {}",
                config.server_binary.display(),
                server.display()
            )
        })?;
        fs::copy("/usr/bin/true", &viewer).context("install isolated no-op gtl-viewer")?;

        let stdout_path = logs.join("server.stdout.log");
        let stderr_path = logs.join("server.stderr.log");
        let stdout = File::create(&stdout_path)
            .with_context(|| format!("create {}", stdout_path.display()))?;
        let stderr = File::create(&stderr_path)
            .with_context(|| format!("create {}", stderr_path.display()))?;
        let mut command = server_command(&server, config.profiler)?;
        command
            .env("GIT_TOOLS_DATA_DIR", &data_root)
            .env("GIT_TOOLS_CONFIG", &settings)
            .env("RUST_LOG", "warn")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        let child = command.spawn().context("start isolated release server")?;
        Ok(Self {
            child,
            data_root,
            stderr_path,
            stopped: false,
        })
    }

    #[must_use]
    pub fn process_id(&self) -> u32 {
        self.child.id()
    }

    #[must_use]
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub fn ensure_running(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait().context("poll release server")? {
            return self.unexpected_exit(status);
        }
        Ok(())
    }

    pub fn stop(mut self) -> Result<()> {
        self.terminate()?;
        self.stopped = true;
        Ok(())
    }

    fn terminate(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait().context("poll release server")? {
            return self.expected_exit(status);
        }
        send_terminate(self.child.id())?;
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().context("wait for release server")? {
                return self.expected_exit(status);
            }
            if started.elapsed() >= SHUTDOWN_TIMEOUT {
                self.child
                    .kill()
                    .context("kill unresponsive release server")?;
                let status = self.child.wait().context("reap killed release server")?;
                bail!(
                    "release server exceeded the {}-second shutdown bound (exit {})",
                    SHUTDOWN_TIMEOUT.as_secs(),
                    status.code().unwrap_or(-1)
                );
            }
            std::thread::sleep(SHUTDOWN_POLL_DELAY);
        }
    }

    fn expected_exit(&self, status: ExitStatus) -> Result<()> {
        if status.success() {
            Ok(())
        } else {
            self.unexpected_exit(status)
        }
    }

    fn unexpected_exit(&self, status: ExitStatus) -> Result<()> {
        let diagnostic = bounded_file_diagnostic(&self.stderr_path)
            .unwrap_or_else(|error| format!("stderr unavailable: {error:#}"));
        bail!(
            "release server exited with {}: {diagnostic}",
            status.code().unwrap_or(-1)
        )
    }
}

impl Drop for ReleaseServerProcess {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn server_command(server: &Path, profiler: Option<MassifProfiler<'_>>) -> Result<Command> {
    let Some(profiler) = profiler else {
        return Ok(Command::new(server));
    };
    let parent = profiler
        .output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("Massif output path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("create Massif output directory {}", parent.display()))?;
    let mut command = Command::new(profiler.valgrind_binary);
    command.args([
        "--tool=massif".to_owned(),
        format!("--massif-out-file={}", profiler.output_path.display()),
        "--time-unit=B".to_owned(),
        "--detailed-freq=10".to_owned(),
        "--max-snapshots=200".to_owned(),
        "--threshold=0.1".to_owned(),
        server.to_string_lossy().into_owned(),
    ]);
    Ok(command)
}

fn send_terminate(process_id: u32) -> Result<()> {
    let process_id = i32::try_from(process_id).context("release server PID exceeds i32")?;
    // kill sends one signal to the exact child PID and does not dereference caller-owned memory.
    let result = unsafe { libc::kill(process_id, libc::SIGTERM) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("send SIGTERM to release server")
    }
}

fn bounded_file_diagnostic(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let end = bytes.len().min(DIAGNOSTIC_BYTES_MAX);
    let suffix = if bytes.len() > end {
        "...[truncated]"
    } else {
        ""
    };
    Ok(format!(
        "{}{suffix}",
        String::from_utf8_lossy(&bytes[..end]).trim()
    ))
}
