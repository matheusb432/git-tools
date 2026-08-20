use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};

use crate::{process, task::Step};

const SERVICE_NAME: &str = "gtl-server.service";

pub(super) fn install(server: &Path) -> Result<Option<PathBuf>> {
    let Some(config_home) = xdg_config_home()? else {
        return Ok(None);
    };
    let path = service_path(&config_home);
    write_service(&path, server)
        .with_context(|| format!("installing gtl-server user service at {}", path.display()))?;
    systemctl(
        "reload gtl-server user service",
        ["--user", "daemon-reload"],
    )?;
    systemctl(
        "enable gtl-server user service",
        ["--user", "enable", SERVICE_NAME],
    )?;
    systemctl(
        "restart gtl-server user service",
        ["--user", "restart", SERVICE_NAME],
    )?;
    Ok(Some(path))
}

pub(super) fn uninstall() -> Result<Option<PathBuf>> {
    let Some(config_home) = xdg_config_home()? else {
        return Ok(None);
    };
    let path = service_path(&config_home);
    if !path.exists() {
        return Ok(None);
    }
    systemctl(
        "stop gtl-server user service",
        ["--user", "disable", "--now", SERVICE_NAME],
    )?;
    fs::remove_file(&path)
        .with_context(|| format!("removing gtl-server user service at {}", path.display()))?;
    systemctl("reload user services", ["--user", "daemon-reload"])?;
    Ok(Some(path))
}

fn systemctl<const N: usize>(label: &str, arguments: [&str; N]) -> Result<()> {
    process::run_step(&Step::new(label, "systemctl", arguments))
}

fn xdg_config_home() -> Result<Option<PathBuf>> {
    if env::consts::OS != "linux" {
        return Ok(None);
    }
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(Some(PathBuf::from(config_home)));
    }
    let home = env::var_os("HOME")
        .context("HOME is not set; cannot install the gtl-server user service")?;
    Ok(Some(PathBuf::from(home).join(".config")))
}

fn service_path(config_home: &Path) -> PathBuf {
    config_home.join("systemd").join("user").join(SERVICE_NAME)
}

fn write_service(path: &Path, server: &Path) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let contents = service_unit(server);
    if fs::read(path).is_ok_and(|existing| existing == contents.as_bytes()) {
        return Ok(());
    }
    let temporary = parent.join(format!(".{SERVICE_NAME}.{}.tmp", std::process::id()));
    fs::write(&temporary, contents)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

fn service_unit(server: &Path) -> String {
    let executable = systemd_exec_argument(server);
    format!(
        "\
[Unit]\n\
Description=Git Tools local gRPC server\n\
\n\
[Service]\n\
Type=simple\n\
ExecStart={executable}\n\
Restart=on-failure\n\
RestartSec=2s\n\
UMask=0077\n\
\n\
[Install]\n\
WantedBy=default.target\n"
    )
}

fn systemd_exec_argument(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('%', "%%")
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_unit_runs_one_restartable_private_process() {
        let unit = service_unit(Path::new("/home/dev/.local/bin/gtl-server"));

        assert!(unit.contains("ExecStart=\"/home/dev/.local/bin/gtl-server\""));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("UMask=0077"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn service_unit_escapes_systemd_specifiers_and_quotes() {
        let unit = service_unit(Path::new("/home/100%/git \"tools\"/gtl-server"));

        assert!(unit.contains("/home/100%%/git \\\"tools\\\"/gtl-server"));
    }

    #[test]
    fn write_service_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = service_path(directory.path());
        let server = Path::new("/home/dev/.local/bin/gtl-server");

        write_service(&path, server).unwrap();
        let first = fs::read(&path).unwrap();
        write_service(&path, server).unwrap();

        assert_eq!(fs::read(path).unwrap(), first);
    }
}
