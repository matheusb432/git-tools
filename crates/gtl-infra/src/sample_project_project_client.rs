//! Async sample_project project adapter backed by `sample_project project list --json`.

use std::{ffi::OsString, path::PathBuf, process::Stdio, time::Duration};

use directories::BaseDirs;
use gtl_application::ports::{ProjectClient, ProjectClientError};
use gtl_models::managed::ManagedRepo;
use serde::Deserialize;
use tokio::{
    io::AsyncReadExt as _,
    process::{Child, Command},
    time::timeout,
};

const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
const OUTPUT_BYTES_MAX: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct SampleProjectClient {
    binary: OsString,
    home: Option<PathBuf>,
    wait_timeout: Duration,
}

impl SampleProjectClient {
    #[must_use]
    pub fn new(binary: impl Into<OsString>, home: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            home: Some(home.into()),
            wait_timeout: DEFAULT_WAIT_TIMEOUT,
        }
    }

    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            binary: "sample_project".into(),
            home: BaseDirs::new().map(|directories| directories.home_dir().to_path_buf()),
            wait_timeout: DEFAULT_WAIT_TIMEOUT,
        }
    }

    /// Lists the managed repositories from sample_project's active project catalogue.
    ///
    /// # Errors
    /// Returns an error when sample_project cannot be executed or its JSON output does not match the
    /// project-list contract.
    pub async fn list_projects(&self) -> Result<Vec<ManagedRepo>, ProjectClientError> {
        let output = self.output().await?;
        parse_projects_json(&output, self.home.as_deref())
    }

    async fn output(&self) -> Result<Vec<u8>, ProjectClientError> {
        let mut child = Command::new(&self.binary)
            .args(["project", "list", "--json"])
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ProjectClientError::Unavailable {
                message: format!("starting `sample_project project list --json`: {error}"),
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ProjectClientError::Unavailable {
                message: "sample_project stdout pipe is unavailable".into(),
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ProjectClientError::Unavailable {
                message: "sample_project stderr pipe is unavailable".into(),
            })?;
        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();
        let process = async {
            let limit = u64::try_from(OUTPUT_BYTES_MAX).unwrap_or(u64::MAX) + 1;
            let mut bounded_stdout = stdout.take(limit);
            let mut bounded_stderr = stderr.take(limit);
            let stdout_read = bounded_stdout.read_to_end(&mut stdout_bytes);
            let stderr_read = bounded_stderr.read_to_end(&mut stderr_bytes);
            let wait = child.wait();
            tokio::join!(stdout_read, stderr_read, wait)
        };

        let Ok((stdout_read, stderr_read, status)) = timeout(self.wait_timeout, process).await
        else {
            return Err(ProjectClientError::Unavailable {
                message: stop_child(
                    &mut child,
                    format!(
                        "`sample_project project list --json` did not return within {}ms",
                        self.wait_timeout.as_millis()
                    ),
                )
                .await,
            });
        };
        stdout_read.map_err(|error| ProjectClientError::Unavailable {
            message: format!("reading sample_project stdout: {error}"),
        })?;
        stderr_read.map_err(|error| ProjectClientError::Unavailable {
            message: format!("reading sample_project stderr: {error}"),
        })?;
        if stdout_bytes.len() > OUTPUT_BYTES_MAX {
            return Err(ProjectClientError::InvalidData {
                message: format!("stdout exceeds {OUTPUT_BYTES_MAX} bytes"),
            });
        }
        if stderr_bytes.len() > OUTPUT_BYTES_MAX {
            return Err(ProjectClientError::Unavailable {
                message: format!("stderr exceeds {OUTPUT_BYTES_MAX} bytes"),
            });
        }
        let status = status.map_err(|error| ProjectClientError::Unavailable {
            message: format!("waiting for sample_project: {error}"),
        })?;
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr_bytes);
            let detail = stderr.trim();
            let detail = if detail.is_empty() {
                "no error output"
            } else {
                detail
            };
            return Err(ProjectClientError::Unavailable {
                message: format!("`sample_project project list --json` exited with {status}: {detail}"),
            });
        }

        Ok(stdout_bytes)
    }
}

impl Default for SampleProjectClient {
    fn default() -> Self {
        Self::from_environment()
    }
}

impl ProjectClient for SampleProjectClient {
    async fn list_projects(&self) -> Result<Vec<ManagedRepo>, ProjectClientError> {
        SampleProjectClient::list_projects(self).await
    }
}

async fn stop_child(child: &mut Child, message: impl Into<String>) -> String {
    let message = message.into();
    let kill = child.kill().await;
    let reap = child.wait().await;

    match (kill, reap) {
        (Ok(()), Ok(_)) => message,
        (kill, reap) => format!("{message}; kill: {kill:?}; reap: {reap:?}"),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SampleProject {
    #[serde(rename = "id")]
    _id: String,
    title: String,
    #[serde(rename = "mux_session_name")]
    _mux_session_name: String,
    source: SampleProjectSource,
    git_remote: Option<String>,
    #[serde(rename = "is_paused")]
    _is_paused: bool,
    #[serde(rename = "affiliation")]
    _affiliation: String,
    #[serde(rename = "color")]
    _color: Option<String>,
    #[serde(rename = "groups")]
    _groups: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SampleProjectSource {
    #[serde(rename = "kind")]
    _kind: DirectoryProjectSource,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum DirectoryProjectSource {
    Directory,
}

fn parse_projects_json(
    raw: &[u8],
    home: Option<&std::path::Path>,
) -> Result<Vec<ManagedRepo>, ProjectClientError> {
    let projects = serde_json::from_slice::<Vec<SampleProject>>(raw).map_err(|error| {
        ProjectClientError::InvalidData {
            message: format!("parsing JSON: {error}"),
        }
    })?;

    projects
        .into_iter()
        .map(|project| {
            let path = expand_project_path(&project.source.value, home)?;
            Ok(ManagedRepo {
                name: project.title,
                path,
                remote: project.git_remote.unwrap_or_default(),
            })
        })
        .collect()
}

fn expand_project_path(
    value: &str,
    home: Option<&std::path::Path>,
) -> Result<PathBuf, ProjectClientError> {
    let path = if value == "~" {
        home.map(PathBuf::from)
    } else if let Some(relative) = value.strip_prefix("~/") {
        home.map(|home| home.join(relative))
    } else {
        Some(PathBuf::from(value))
    }
    .ok_or_else(|| ProjectClientError::Unavailable {
        message: "resolving the home directory for an sample_project project source".into(),
    })?;

    if !path.is_absolute() {
        return Err(ProjectClientError::InvalidData {
            message: format!("project source is not absolute: {value}"),
        });
    }
    Ok(path)
}

#[cfg(all(test, unix))]
mod tests {
    use std::{os::unix::fs::PermissionsExt as _, path::Path, time::Instant};

    use super::*;

    const FIXTURE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
    static sample_project_PROCESS_FIXTURE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test]
    async fn rejects_oversized_output_and_reaps_sample_project() -> anyhow::Result<()> {
        let _fixture_guard = lock_process_fixture().await?;
        let directory = tempfile::tempdir()?;
        let marker = directory.path().join("sample_project.pid");
        let output = directory.path().join("projects.json");
        std::fs::write(&output, "x".repeat(OUTPUT_BYTES_MAX + 1))?;
        let script = directory.path().join("sample_project");
        write_executable(
            &script,
            &format!(
                "#!/bin/sh\nprintf '%s' $$ > '{}'\nexec cat '{}'\n",
                marker.display(),
                output.display()
            ),
        )?;
        let client = SampleProjectClient::new(script.as_os_str(), "/home/u");

        let Err(error) = client.list_projects().await else {
            anyhow::bail!("oversized sample_project output was accepted");
        };

        assert!(matches!(error, ProjectClientError::InvalidData { .. }));
        assert_process_reaped(&marker)?;
        Ok(())
    }

    #[tokio::test]
    async fn timeout_kills_and_reaps_sample_project() -> anyhow::Result<()> {
        let _fixture_guard = lock_process_fixture().await?;
        let directory = tempfile::tempdir()?;
        let marker = directory.path().join("sample_project.pid");
        let script = directory.path().join("sample_project");
        write_executable(
            &script,
            &format!(
                "#!/bin/sh\nprintf '%s' $$ > '{}'\nexec sleep 30\n",
                marker.display()
            ),
        )?;
        let client = SampleProjectClient {
            binary: script.into_os_string(),
            home: Some("/home/u".into()),
            wait_timeout: Duration::from_millis(100),
        };

        let started = Instant::now();
        let Err(error) = client.list_projects().await else {
            anyhow::bail!("timed out sample_project command succeeded");
        };

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(error, ProjectClientError::Unavailable { .. }));
        assert_process_reaped(&marker)?;
        Ok(())
    }

    fn write_executable(path: &Path, contents: &str) -> std::io::Result<()> {
        std::fs::write(path, contents)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    }

    fn assert_process_reaped(marker: &Path) -> anyhow::Result<()> {
        let pid = std::fs::read_to_string(marker)?.trim().parse::<u32>()?;
        anyhow::ensure!(!Path::new(&format!("/proc/{pid}")).exists());
        Ok(())
    }

    async fn lock_process_fixture() -> anyhow::Result<tokio::sync::MutexGuard<'static, ()>> {
        tokio::time::timeout(FIXTURE_LOCK_TIMEOUT, sample_project_PROCESS_FIXTURE.lock())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for the sample_project process fixture"))
    }
}
