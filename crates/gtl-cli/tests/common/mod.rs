use std::{
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};

pub struct ServerHarness {
    _data_root: tempfile::TempDir,
}

impl ServerHarness {
    pub fn start(settings_path: Option<&Path>, project_repository: Option<&Path>) -> Result<Self> {
        let data_root = tempfile::tempdir().context("create gtl-server data root")?;
        // Each integration-test file is a separate process with one server test, so these
        // process-wide variables are established before either the server thread or CLI child.
        unsafe {
            std::env::set_var("GIT_TOOLS_DATA_DIR", data_root.path());
            if let Some(settings_path) = settings_path {
                std::env::set_var("GIT_TOOLS_CONFIG", settings_path);
            } else {
                std::env::remove_var("GIT_TOOLS_CONFIG");
            }
        }
        let (failure_tx, failure_rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || report_server_failure(&failure_tx));

        wait_for_server_endpoint(data_root.path(), &failure_rx)?;
        if let Some(repository) = project_repository {
            register_project("RP", "repo", repository)?;
        }
        Ok(Self {
            _data_root: data_root,
        })
    }
}

fn report_server_failure(failure: &std::sync::mpsc::SyncSender<String>) {
    if let Err(error) = run_server() {
        drop(failure.send(error.to_string()));
    }
}

fn run_server() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build gtl-server test runtime")?;
    runtime
        .block_on(gtl_server::run())
        .context("run gtl-server for CLI integration test")
}

fn wait_for_server_endpoint(
    data_root: &Path,
    failure: &std::sync::mpsc::Receiver<String>,
) -> Result<()> {
    let endpoint = gtl_local_transport::LocalEndpoint::from_root(data_root)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build server readiness runtime")?;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(15) {
        check_server_failure(failure)?;
        if runtime
            .block_on(gtl_client::GtlClient::connect(&endpoint))
            .is_ok()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    bail!("gtl-server did not become ready within 15 seconds")
}

fn check_server_failure(failure: &std::sync::mpsc::Receiver<String>) -> Result<()> {
    match failure.try_recv() {
        Ok(error) => bail!(error),
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            bail!("gtl-server test thread exited before publishing its endpoint");
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => Ok(()),
    }
}

pub fn register_project(id: &str, title: &str, repository: &Path) -> Result<()> {
    use gtl_wire::v1;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client = gtl_client::GtlClient::connect_local().await?;
        client
            .create_project(v1::CreateProjectRequest {
                project_id: id.into(),
                project: Some(v1::ProjectCreation {
                    title: title.into(),
                    source: Some(v1::ProjectSource {
                        source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                            path: repository.to_string_lossy().into_owned(),
                        })),
                    }),
                    git_remote: None,
                    color: None,
                    groups: Vec::new(),
                    include_in_full_export: None,
                }),
            })
            .await?;
        Ok(())
    })
}
