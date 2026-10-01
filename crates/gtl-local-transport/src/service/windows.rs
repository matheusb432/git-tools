use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::{SERVER_INSTALL_COMMAND, State, checked, environment, process};

const SCRIPT: &str = include_str!("windows.ps1");

pub(super) struct Registration {
    powershell: PathBuf,
}

impl Registration {
    pub(super) async fn startup_enabled(&self) -> anyhow::Result<bool> {
        Ok(self.invoke("Enabled", None).await?.trim() == "true")
    }
    pub(super) fn discover() -> anyhow::Result<Self> {
        let root = PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot is not set")?);
        let powershell = root.join("System32/WindowsPowerShell/v1.0/powershell.exe");
        ensure!(
            powershell.is_absolute() && powershell.is_file(),
            "Windows PowerShell is unavailable at {}",
            powershell.display()
        );
        Ok(Self { powershell })
    }

    async fn invoke(&self, action: &str, server: Option<&Path>) -> anyhow::Result<String> {
        let script = format!(
            "try {{\n{SCRIPT}\nexit 0\n}} catch {{ [Console]::Error.WriteLine($_.ToString()); exit 1 }}"
        );
        let encoded = STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let mut command = process::Command::new(&self.powershell);
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &encoded,
            ])
            .env("GTL_SERVER_ACTION", action)
            .env("GTL_SERVER_INSTALL_COMMAND", SERVER_INSTALL_COMMAND)
            .env(
                "GTL_SERVER_ENVIRONMENT",
                serde_json::to_string(&environment())?,
            );
        if let Some(server) = server {
            command.env("GTL_SERVER_PROGRAM", server);
        }
        checked(&mut command)
            .await
            .with_context(|| format!("gtl-server scheduled task {action} failed"))
    }

    pub(super) async fn status(&self) -> anyhow::Result<State> {
        match self.invoke("Status", None).await?.trim() {
            "running" => Ok(State::Running),
            "stopped" => Ok(State::Stopped),
            "not installed" => Ok(State::NotInstalled),
            "failed" => Ok(State::Failed),
            other => bail!("unknown gtl-server task state: {other}"),
        }
    }

    pub(super) async fn diagnostic_command(&self) -> anyhow::Result<process::Command> {
        let configuration: serde_json::Value =
            serde_json::from_str(&self.invoke("Configuration", None).await?)?;
        let program = configuration["program"].as_str().with_context(|| {
            format!("scheduled task has no diagnostic executable; run `{SERVER_INSTALL_COMMAND}`")
        })?;
        let mut command = process::Command::new(program);
        for name in super::ENVIRONMENT_NAMES {
            command.env_remove(name);
        }
        let environment: Vec<(String, String)> =
            serde_json::from_value(configuration["environment"].clone())?;
        command.envs(environment);
        Ok(command)
    }

    pub(super) async fn failure_detail(&self) -> anyhow::Result<String> {
        self.invoke("Failure", None).await
    }

    pub(super) async fn stop(&self) -> anyhow::Result<()> {
        self.invoke("Stop", None).await?;
        Ok(())
    }
    pub(super) async fn start(&self) -> anyhow::Result<()> {
        self.invoke("Start", None).await?;
        Ok(())
    }
    pub(super) async fn install(&self, server: &Path) -> anyhow::Result<()> {
        self.invoke("Install", Some(server)).await?;
        Ok(())
    }
    pub(super) async fn uninstall(&self) -> anyhow::Result<()> {
        self.invoke("Uninstall", None).await?;
        Ok(())
    }
}
