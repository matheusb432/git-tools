use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use service_manager::ServiceLevel;
use service_manager::{
    RestartPolicy, ServiceInstallCtx, ServiceLabel, ServiceManager, ServiceStartCtx, ServiceStatus,
    ServiceStatusCtx, ServiceStopCtx, ServiceUninstallCtx,
};

const SERVICE_LABEL: &str = "gtl-server";

pub(super) struct PreparedService {
    manager: Box<dyn ServiceManager>,
    label: ServiceLabel,
    install_context: ServiceInstallCtx,
}

impl PreparedService {
    pub(super) fn unregister_if_installed(&self) -> Result<bool> {
        unregister_if_installed(self.manager.as_ref(), &self.label)
    }

    pub(super) fn install_and_start(self) -> Result<()> {
        self.manager
            .install(self.install_context)
            .context("registering the gtl-server user service")?;
        self.manager
            .start(ServiceStartCtx { label: self.label })
            .context("starting the gtl-server user service")
    }
}

pub(super) fn prepare(program: &Path) -> Result<Option<PreparedService>> {
    let Some(manager) = user_service_manager()? else {
        return Ok(None);
    };
    let install_context = service_install_context(program.to_path_buf())?;
    let label = install_context.label.clone();
    Ok(Some(PreparedService {
        manager,
        label,
        install_context,
    }))
}

pub(super) fn uninstall() -> Result<bool> {
    let Some(manager) = user_service_manager()? else {
        return Ok(false);
    };
    unregister_if_installed(manager.as_ref(), &service_label()?)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn user_service_manager() -> Result<Option<Box<dyn ServiceManager>>> {
    let mut manager = <dyn ServiceManager>::native()
        .context("detecting the platform service manager for gtl-server")?;
    manager
        .set_level(ServiceLevel::User)
        .context("selecting user-level service management for gtl-server")?;
    ensure!(
        manager
            .available()
            .context("checking the platform service manager for gtl-server")?,
        "the platform service manager for gtl-server is unavailable"
    );
    Ok(Some(manager))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn user_service_manager() -> Result<Option<Box<dyn ServiceManager>>> {
    Ok(None)
}

fn service_label() -> Result<ServiceLabel> {
    SERVICE_LABEL
        .parse()
        .context("parsing the static gtl-server service label")
}

fn unregister_if_installed(manager: &dyn ServiceManager, label: &ServiceLabel) -> Result<bool> {
    let status = manager
        .status(ServiceStatusCtx {
            label: label.clone(),
        })
        .context("reading the existing gtl-server user service status")?;
    if status == ServiceStatus::NotInstalled {
        return Ok(false);
    }
    if status == ServiceStatus::Running {
        manager
            .stop(ServiceStopCtx {
                label: label.clone(),
            })
            .context("stopping the existing gtl-server user service")?;
    }
    manager
        .uninstall(ServiceUninstallCtx {
            label: label.clone(),
        })
        .context("unregistering the existing gtl-server user service")?;
    Ok(true)
}

fn service_install_context(program: PathBuf) -> Result<ServiceInstallCtx> {
    ensure!(
        program.is_absolute(),
        "gtl-server service executable path must be absolute: {}",
        program.display()
    );
    #[cfg(target_os = "linux")]
    let contents = Some(service_unit(&program));
    #[cfg(not(target_os = "linux"))]
    let contents = None;
    #[cfg(target_os = "macos")]
    let environment = Some(macos_environment(
        &program,
        std::env::var_os("PATH").as_deref(),
    )?);
    #[cfg(not(target_os = "macos"))]
    let environment = None;
    Ok(ServiceInstallCtx {
        label: service_label()?,
        contents,
        program,
        args: Vec::new(),
        username: None,
        working_directory: None,
        environment,
        autostart: true,
        restart_policy: RestartPolicy::OnFailure {
            delay_secs: Some(2),
            max_retries: None,
            reset_after_secs: None,
        },
    })
}

#[cfg(any(target_os = "macos", all(test, unix)))]
fn macos_environment(
    program: &Path,
    inherited_path: Option<&std::ffi::OsStr>,
) -> Result<Vec<(String, String)>> {
    let bindir = program.parent().context("gtl-server install directory")?;
    // launchd does not read the developer's shell startup files.
    let paths = std::iter::once(bindir.to_path_buf())
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
    let path = std::env::join_paths(paths)
        .context("constructing the gtl-server launchd PATH")?
        .into_string()
        .map_err(|_| anyhow::anyhow!("the gtl-server launchd PATH must be valid UTF-8"))?;
    Ok(vec![("PATH".to_owned(), path)])
}

#[cfg(target_os = "linux")]
fn service_unit(server: &Path) -> String {
    let executable = systemd_exec_argument(server);
    format!(
        "\
[Unit]\n\
Description=Git Tools local gRPC server\n\
After=graphical-session.target\n\
\n\
[Service]\n\
Type=simple\n\
ExecStart={executable}\n\
Restart=on-failure\n\
RestartSec=2s\n\
UMask=0077\n\
\n\
[Install]\n\
WantedBy=graphical-session.target\n"
    )
}

#[cfg(target_os = "linux")]
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
    fn install_context_registers_a_restartable_login_service() {
        let server = PathBuf::from("/home/dev/.local/bin/gtl-server");
        let context = service_install_context(server.clone()).unwrap();

        assert_eq!(context.label.to_qualified_name(), SERVICE_LABEL);
        assert_eq!(context.program, server);
        assert!(context.args.is_empty());
        assert!(context.autostart);
        assert_eq!(
            context.restart_policy,
            RestartPolicy::OnFailure {
                delay_secs: Some(2),
                max_retries: None,
                reset_after_secs: None,
            }
        );
    }

    #[test]
    fn install_context_rejects_a_relative_server_path() {
        let error = service_install_context(PathBuf::from("gtl-server")).unwrap_err();

        assert!(error.to_string().contains("must be absolute"));
    }

    #[cfg(unix)]
    #[test]
    fn launchd_path_preserves_developer_tools_and_supplies_login_defaults() {
        let server = Path::new("/Users/developer/Local Tools/bin/gtl-server");
        for inherited in [None, Some(std::ffi::OsStr::new("/custom/git/bin:relative"))] {
            let environment = macos_environment(server, inherited).unwrap();
            let (name, value) = &environment[0];
            assert_eq!(name, "PATH");
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

    #[cfg(target_os = "linux")]
    #[test]
    fn service_unit_runs_one_restartable_private_process() {
        let unit = service_unit(Path::new("/home/dev/.local/bin/gtl-server"));

        assert!(unit.contains("ExecStart=\"/home/dev/.local/bin/gtl-server\""));
        assert!(unit.contains("After=graphical-session.target"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("UMask=0077"));
        assert!(unit.contains("WantedBy=graphical-session.target"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn service_unit_escapes_systemd_specifiers_and_quotes() {
        let unit = service_unit(Path::new("/home/100%/git \"tools\"/gtl-server"));

        assert!(unit.contains("/home/100%%/git \\\"tools\\\"/gtl-server"));
    }
}
