use std::{
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use gtl_benchmarks::release_server::{MassifProfiler, ReleaseServerConfig, ReleaseServerProcess};
use gtl_client::GtlClient;
use gtl_local_transport::LocalEndpoint;
use gtl_wire::{
    v1::viewer_service_client::ViewerServiceClient, viewer::VIEWER_ROW_MAX_ENCODED_BYTES,
};
use tonic::transport::Channel;

use super::environment::DriverConfig;

const READY_TIMEOUT: Duration = Duration::from_secs(60);
const READY_RETRY_DELAY: Duration = Duration::from_millis(10);
const SETTINGS: &str = "layout = \"unified\"\ndensity = \"compact\"\n\n[diff]\nexclude = []\n";

pub type BenchmarkViewerClient = ViewerServiceClient<Channel>;

pub struct ServerClients {
    pub application: GtlClient,
    pub viewer: BenchmarkViewerClient,
}

pub struct ServerProcess {
    process: ReleaseServerProcess,
}

impl ServerProcess {
    pub fn start(config: &DriverConfig, root: &Path) -> Result<Self> {
        let profiler = config.massif.as_ref().map(|massif| MassifProfiler {
            valgrind_binary: &massif.valgrind_binary,
            output_path: &massif.output_path,
        });
        Ok(Self {
            process: ReleaseServerProcess::start(
                ReleaseServerConfig {
                    server_binary: &config.server_binary,
                    settings: SETTINGS,
                    profiler,
                },
                root,
            )?,
        })
    }

    pub fn process_id(&self) -> u32 {
        self.process.process_id()
    }

    pub async fn connect(&mut self) -> Result<ServerClients> {
        let endpoint = LocalEndpoint::from_root(self.process.data_root())
            .context("resolve isolated server endpoint")?;
        wait_for_server_clients(&mut self.process, &endpoint).await
    }

    pub fn stop(self) -> Result<()> {
        self.process.stop()
    }
}

async fn wait_for_server_clients(
    process: &mut ReleaseServerProcess,
    endpoint: &LocalEndpoint,
) -> Result<ServerClients> {
    let started = Instant::now();
    loop {
        process.ensure_running()?;
        if let Ok(application) = GtlClient::connect(endpoint).await
            && let Ok(viewer) = connect_viewer_client(endpoint).await
        {
            return Ok(ServerClients {
                application,
                viewer,
            });
        }
        ensure!(
            started.elapsed() < READY_TIMEOUT,
            "release server did not become healthy within {} seconds",
            READY_TIMEOUT.as_secs()
        );
        tokio::time::sleep(READY_RETRY_DELAY).await;
    }
}

async fn connect_viewer_client(endpoint: &LocalEndpoint) -> Result<BenchmarkViewerClient> {
    let channel = endpoint
        .connect(Duration::from_secs(1), Duration::from_secs(30))
        .await
        .context("connect isolated viewer channel")?;
    Ok(ViewerServiceClient::new(channel)
        .max_decoding_message_size(VIEWER_ROW_MAX_ENCODED_BYTES + 64 * 1_024))
}
