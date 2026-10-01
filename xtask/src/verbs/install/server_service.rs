use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use gtl_local_transport::service::{ServerService, State};

pub(super) struct PreparedService {
    runtime: tokio::runtime::Runtime,
    service: ServerService,
    program: PathBuf,
}

impl PreparedService {
    pub(super) fn unregister_if_installed(&self) -> Result<bool> {
        unregister_if_installed(&self.runtime, &self.service)
    }

    pub(super) fn install_and_start(self) -> Result<()> {
        self.runtime.block_on(async {
            self.service.install(&self.program).await?;
            self.service.start().await
        })
    }
}

pub(super) fn prepare(program: &Path) -> Result<Option<PreparedService>> {
    let Some((runtime, service)) = user_service()? else {
        return Ok(None);
    };
    ensure!(
        program.is_absolute(),
        "gtl-server service executable path must be absolute: {}",
        program.display()
    );
    Ok(Some(PreparedService {
        runtime,
        service,
        program: program.to_path_buf(),
    }))
}

fn user_service() -> Result<Option<(tokio::runtime::Runtime, ServerService)>> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let service = ServerService::discover()?;
    Ok(Some((runtime, service)))
}

pub(super) fn uninstall() -> Result<bool> {
    let Some((runtime, service)) = user_service()? else {
        return Ok(false);
    };
    unregister_if_installed(&runtime, &service)
}

fn unregister_if_installed(
    runtime: &tokio::runtime::Runtime,
    service: &ServerService,
) -> Result<bool> {
    let installed = runtime.block_on(service.status())? != State::NotInstalled;
    if installed {
        runtime.block_on(service.uninstall())?;
    }
    Ok(installed)
}
