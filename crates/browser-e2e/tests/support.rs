use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Output, Stdio},
    thread,
    time::Duration,
};

use anyhow::{Context as _, Result, bail, ensure};
use browser_e2e::{
    browser::{self, OPERATION_TIMEOUT, Session, operation},
    evidence::Recording,
};
use command_group::{CommandGroup, GroupChild};
use playwright_rs::protocol::{AriaRole, ClickOptions, GetByRoleOptions, Locator, Page, Viewport};

const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const WAIT_INTERVAL: Duration = Duration::from_millis(100);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const PROCESS_REAP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Spec {
    pub session: Session,
    recording: Recording,
}

impl Spec {
    pub async fn start(name: &'static str) -> Result<Self> {
        verify_runtime_environment()?;
        let session = browser::open().await?;
        let recording = Recording::new(&session, name);
        Ok(Self { session, recording })
    }

    pub async fn finish(self, outcome: Result<()>) -> Result<()> {
        let evidence_result = self.recording.finish(outcome.is_ok()).await;
        let cleanup_result = self.session.finish().await;
        if let Err(failure) = outcome {
            if let Err(error) = evidence_result {
                eprintln!("browser evidence failed: {error:#}");
            }
            if let Err(error) = cleanup_result {
                eprintln!("browser cleanup failed: {error:#}");
            }
            Err(failure)
        } else {
            evidence_result?;
            cleanup_result
        }
    }
}

fn verify_runtime_environment() -> Result<()> {
    ensure!(
        env::var("GTL_E2E_RUNTIME_ISOLATED").as_deref() == Ok("1"),
        "browser E2E must run through the typed xtask runtime boundary"
    );
    for name in ["CARGO_HOME", "RUSTUP_HOME", "SSH_AUTH_SOCK"] {
        ensure!(
            env::var_os(name).is_none(),
            "browser E2E runtime inherited host-only {name}"
        );
    }
    Ok(())
}

pub async fn repository_with_worktree_change() -> Result<tempfile::TempDir> {
    let repository = tempfile::Builder::new()
        .prefix("gtl-offline-artifact-")
        .tempdir()
        .context("create disposable Git repository")?;
    git(repository.path(), &["init", "-q", "-b", "main"]).await?;
    git(repository.path(), &["config", "user.name", "Browser E2E"]).await?;
    git(
        repository.path(),
        &["config", "user.email", "browser-e2e@example.invalid"],
    )
    .await?;
    std::fs::write(repository.path().join("artifact.txt"), "base\n")
        .context("write Git fixture")?;
    git(repository.path(), &["add", "artifact.txt"]).await?;
    git(repository.path(), &["commit", "-q", "-m", "base"]).await?;
    std::fs::write(repository.path().join("artifact.txt"), "base\nchanged\n")
        .context("write changed Git fixture")?;
    Ok(repository)
}

pub async fn render_raw_diff(repository: &tempfile::TempDir) -> Result<String> {
    let cli_binary = env::var_os("GTL_E2E_CLI_BINARY")
        .map(PathBuf::from)
        .context("GTL_E2E_CLI_BINARY is required for browser E2E")?;
    let mut command = Command::new(&cli_binary);
    command
        .args(["diff", "--raw"])
        .current_dir(repository.path());
    let output = command_output(command, "git-tools diff --raw").await?;
    ensure_success(&output, "git-tools diff --raw")?;
    let urls = String::from_utf8(output.stdout)
        .context("decode git-tools diff --raw stdout")?
        .lines()
        .filter(|line| line.starts_with("file://"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    match urls.as_slice() {
        [url] => Ok(url.clone()),
        [] => bail!("git-tools diff --raw printed no file:// artifact URL"),
        _ => bail!("git-tools diff --raw printed multiple file:// artifact URLs"),
    }
}

pub async fn set_mobile_viewport(page: &Page) -> Result<()> {
    operation("set mobile Chromium viewport", async {
        page.set_viewport_size(Viewport {
            width: 390,
            height: 844,
        })
        .await
        .context("set mobile Chromium viewport")
    })
    .await
}

#[must_use]
pub fn get_button(page: &Page, name: &str) -> Locator {
    page.get_by_role(
        AriaRole::Button,
        Some(GetByRoleOptions::default().name(name).exact(true)),
    )
}

pub async fn wait_until_every_file_is_collapsed(page: &Page) -> Result<()> {
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        let expanded_files = operation("count expanded file sections", async {
            page.locator("details.file[open]")
                .count()
                .await
                .context("count expanded file sections")
        })
        .await?;
        if expanded_files == 0 {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "timed out waiting for all file sections to collapse; {expanded_files} remain open"
            );
        }
        tokio::time::sleep(WAIT_INTERVAL).await;
    }
}

pub async fn count(locator: &Locator, label: &str) -> Result<usize> {
    operation(label, async {
        locator.count().await.with_context(|| label.to_owned())
    })
    .await
}

pub async fn click(locator: &Locator, label: &str) -> Result<()> {
    operation(label, async {
        locator
            .click(
                ClickOptions::builder()
                    .timeout(OPERATION_TIMEOUT.as_secs_f64() * 1_000.0)
                    .build(),
            )
            .await
            .with_context(|| label.to_owned())
    })
    .await
}

pub async fn goto(page: &Page, url: &str) -> Result<()> {
    operation("navigate to offline artifact", async {
        page.goto(
            url,
            playwright_rs::protocol::GotoOptions::new().timeout(OPERATION_TIMEOUT),
        )
        .await
        .context("navigate to offline artifact")
        .map(|_| ())
    })
    .await
}

async fn git(repository: &Path, arguments: &[&str]) -> Result<()> {
    let mut command = Command::new("git");
    command.args(arguments).current_dir(repository);
    let output = command_output(command, &format!("git {}", arguments.join(" "))).await?;
    ensure_success(&output, &format!("git {}", arguments.join(" ")))
}

async fn command_output(mut command: Command, description: &str) -> Result<Output> {
    let description = description.to_owned();
    tokio::task::spawn_blocking(move || command_output_before_deadline(&mut command, &description))
        .await
        .context("join bounded browser E2E process")?
}

fn command_output_before_deadline(command: &mut Command, description: &str) -> Result<Output> {
    let stdout = tempfile::NamedTempFile::new().context("create browser E2E stdout capture")?;
    let stderr = tempfile::NamedTempFile::new().context("create browser E2E stderr capture")?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.reopen().context("open browser E2E stdout capture")?)
        .stderr(stderr.reopen().context("open browser E2E stderr capture")?);
    let mut child = command
        .group_spawn()
        .with_context(|| format!("start {description}"))?;
    let status = match wait_for_exit(&mut child, description) {
        Ok(status) => status,
        Err(error) => {
            return match terminate_and_reap(&mut child, description) {
                Ok(()) => Err(error),
                Err(cleanup_error) => {
                    Err(error.context(format!("process cleanup also failed: {cleanup_error:#}")))
                }
            };
        }
    };
    Ok(Output {
        status,
        stdout: fs::read(stdout.path()).context("read browser E2E stdout capture")?,
        stderr: fs::read(stderr.path()).context("read browser E2E stderr capture")?,
    })
}

fn wait_for_exit(child: &mut GroupChild, description: &str) -> Result<ExitStatus> {
    let deadline = std::time::Instant::now() + PROCESS_TIMEOUT;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("poll {description} process group"))?
        {
            return Ok(status);
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "{description} timed out after {} seconds",
                PROCESS_TIMEOUT.as_secs()
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
    let deadline = std::time::Instant::now() + PROCESS_REAP_TIMEOUT;
    loop {
        if child
            .try_wait()
            .with_context(|| format!("reap {description} process group"))?
            .is_some()
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "{description} process group did not reap within {} seconds",
                PROCESS_REAP_TIMEOUT.as_secs()
            );
        }
        thread::sleep(PROCESS_POLL_INTERVAL);
    }
}

fn ensure_success(output: &Output, description: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "{description} failed (exit {}): {}{}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
