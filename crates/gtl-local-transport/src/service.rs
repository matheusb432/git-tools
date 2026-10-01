//! Native login registration for the private local server.

#[cfg(unix)]
use std::path::PathBuf;
use std::{
    path::Path,
    process::{Output, Stdio},
    time::Duration,
};

use anyhow::{Context as _, bail, ensure};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _},
    process,
    time::timeout,
};

#[cfg(target_os = "linux")]
#[path = "service/linux.rs"]
mod native;
#[cfg(target_os = "macos")]
#[path = "service/macos.rs"]
mod native;
#[cfg(windows)]
#[path = "service/windows.rs"]
mod native;

const SERVER_INSTALL_COMMAND: &str = "gtl server install";
#[cfg(target_os = "linux")]
const SERVER_DOCTOR_COMMAND: &str = "gtl-server doctor";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
const OUTPUT_LIMIT: u64 = 64 * 1024;
const ENVIRONMENT_NAMES: [&str; 7] = [
    "GIT_TOOLS_DATA_DIR",
    "GIT_TOOLS_CONFIG",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "PATH",
    "HOME",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    NotInstalled,
    Stopped,
    Running,
    Failed,
    #[cfg(target_os = "linux")]
    Starting,
}

pub struct ServerService(native::Registration);

impl ServerService {
    pub fn discover() -> anyhow::Result<Self> {
        Ok(Self(native::Registration::discover()?))
    }

    pub async fn status(&self) -> anyhow::Result<State> {
        self.0.status().await
    }

    pub async fn startup_enabled(&self) -> anyhow::Result<bool> {
        self.0.startup_enabled().await
    }
    pub async fn diagnostic_command(&self) -> anyhow::Result<process::Command> {
        self.0.diagnostic_command().await
    }
    pub async fn failure_detail(&self) -> anyhow::Result<String> {
        self.0.failure_detail().await
    }
    pub async fn start(&self) -> anyhow::Result<()> {
        self.0.start().await
    }
    /// Waits for the registered process to exit, retaining login registration.
    pub async fn stop(&self) -> anyhow::Result<()> {
        self.0.stop().await
    }
    /// Registers an existing absolute executable, retaining application data.
    pub async fn install(&self, server: &Path) -> anyhow::Result<()> {
        ensure!(
            server.is_absolute() && server.is_file(),
            "server registration requires an existing absolute executable: {}",
            server.display()
        );
        self.0.install(server).await
    }
    /// Stops the process and removes login registration, retaining binaries and data.
    pub async fn uninstall(&self) -> anyhow::Result<()> {
        self.stop().await?;
        self.0.uninstall().await
    }
}

/// Captures bounded subprocess output and kills a child that exceeds its deadline.
pub async fn output(command: &mut process::Command) -> anyhow::Result<Output> {
    let program = command.as_std().get_program().to_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("starting {}", program.to_string_lossy()))?;
    let stdout = child.stdout.take().context("missing subprocess stdout")?;
    let stderr = child.stderr.take().context("missing subprocess stderr")?;
    let result = timeout(COMMAND_TIMEOUT, async {
        let (status, stdout, stderr) =
            tokio::try_join!(child.wait(), read_output(stdout), read_output(stderr))?;
        Ok::<_, anyhow::Error>(Output {
            status,
            stdout,
            stderr,
        })
    })
    .await;
    if let Ok(result) = result {
        result
    } else {
        let _ = timeout(Duration::from_secs(2), child.kill()).await;
        bail!("{} timed out after 20 seconds", program.to_string_lossy())
    }
}

async fn read_output(reader: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(std::io::Error::other(
            "service command output exceeded 64 KiB",
        ));
    }
    Ok(bytes)
}

async fn checked(command: &mut process::Command) -> anyhow::Result<String> {
    let response = output(command).await?;
    ensure!(
        response.status.success(),
        "service command failed ({}): {}",
        response.status,
        String::from_utf8_lossy(&response.stderr).trim()
    );
    Ok(String::from_utf8(response.stdout)?)
}

#[cfg(target_os = "linux")]
async fn wait_stopped(registration: &native::Registration) -> anyhow::Result<()> {
    timeout(Duration::from_secs(12), async {
        while matches!(
            registration.status().await?,
            State::Running | State::Starting
        ) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("gtl-server did not stop within 12 seconds")?
}

#[cfg(unix)]
fn absolute_environment(name: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name).with_context(|| format!("{name} is not set"))?);
    ensure!(path.is_absolute(), "{name} must be an absolute path");
    Ok(path)
}

fn environment() -> Vec<(String, String)> {
    ENVIRONMENT_NAMES
        .into_iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| (name.to_owned(), value))
        })
        .collect()
}

#[cfg(any(target_os = "macos", all(test, unix)))]
fn macos_environment(
    server: &Path,
    inherited_path: Option<&std::ffi::OsStr>,
) -> anyhow::Result<Vec<(String, String)>> {
    let mut environment = environment();
    environment.retain(|(name, _)| name != "PATH");
    let paths = std::iter::once(server.parent().context("server directory")?.to_path_buf())
        .chain(
            inherited_path
                .into_iter()
                .flat_map(std::env::split_paths)
                .filter(|path| path.is_absolute()),
        )
        .chain(
            [
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin",
            ]
            .map(PathBuf::from),
        );
    let path = std::env::join_paths(paths)?
        .into_string()
        .map_err(|_| anyhow::anyhow!("launchd PATH must be UTF-8"))?;
    environment.push(("PATH".into(), path));
    Ok(environment)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn excessive_subprocess_output_fails_without_waiting_for_the_child() {
        let error = output(process::Command::new("yes").arg("x"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("exceeded 64 KiB"));
    }

    #[test]
    fn launchd_path_preserves_developer_tools_and_supplies_login_defaults() {
        let server = Path::new("/Users/developer/Local Tools/bin/gtl-server");
        for inherited in [None, Some(std::ffi::OsStr::new("/custom/git/bin:relative"))] {
            let environment = macos_environment(server, inherited).unwrap();
            let (_, value) = environment.iter().find(|(name, _)| name == "PATH").unwrap();
            let paths: Vec<_> = std::env::split_paths(value).collect();
            assert_eq!(paths[0], server.parent().unwrap());
            assert_eq!(
                paths.contains(&PathBuf::from("/custom/git/bin")),
                inherited.is_some()
            );
            assert!(paths.iter().all(|path| path.is_absolute()));
            for directory in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
                assert!(paths.contains(&PathBuf::from(directory)));
            }
        }
    }
}
