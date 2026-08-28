use std::{
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use gtl_benchmarks::release_server::{MassifProfiler, ReleaseServerConfig, ReleaseServerProcess};
use gtl_client::GtlClient;
use gtl_local_auth::LocalAuth;
use gtl_wire::{
    v1::viewer_service_client::ViewerServiceClient, viewer::VIEWER_ROW_MAX_ENCODED_BYTES,
};
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    service::{Interceptor, interceptor::InterceptedService},
    transport::Channel,
};

use super::environment::DriverConfig;

const READY_TIMEOUT: Duration = Duration::from_secs(60);
const READY_RETRY_DELAY: Duration = Duration::from_millis(10);
const SETTINGS: &str = "layout = \"unified\"\ndensity = \"compact\"\n\n[diff]\nexclude = []\n";

pub type BenchmarkViewerClient =
    ViewerServiceClient<InterceptedService<Channel, ViewerAuthorization>>;

pub struct ServerClients {
    pub application: GtlClient,
    pub viewer: BenchmarkViewerClient,
}

#[derive(Clone)]
pub struct ViewerAuthorization {
    value: MetadataValue<Ascii>,
}

impl Interceptor for ViewerAuthorization {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        request
            .metadata_mut()
            .insert("authorization", self.value.clone());
        Ok(request)
    }
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
        let auth = LocalAuth::from_data_root(self.process.data_root())
            .context("open isolated server authentication")?;
        let started = Instant::now();
        loop {
            self.process.ensure_running()?;
            if let Ok(application) = GtlClient::connect(&auth).await
                && let Ok(viewer) = connect_viewer_client(&auth).await
            {
                return Ok(ServerClients {
                    application,
                    viewer,
                });
            }
            ensure!(
                started.elapsed() < READY_TIMEOUT,
                "release server did not publish a healthy endpoint within {} seconds",
                READY_TIMEOUT.as_secs()
            );
            tokio::time::sleep(READY_RETRY_DELAY).await;
        }
    }

    pub fn stop(self) -> Result<()> {
        self.process.stop()
    }
}

async fn connect_viewer_client(auth: &LocalAuth) -> Result<BenchmarkViewerClient> {
    let bootstrap = auth
        .load_viewer_bootstrap()
        .context("load isolated viewer bootstrap")?;
    let endpoint = tonic::transport::Endpoint::from_shared(format!(
        "http://{}",
        bootstrap.endpoint().address()
    ))
    .context("build isolated viewer endpoint")?;
    let channel = endpoint
        .connect()
        .await
        .context("connect isolated viewer channel")?;
    let authorization = ViewerAuthorization {
        value: format!("Bearer {}", bootstrap.capability().expose_secret())
            .parse()
            .context("encode isolated viewer capability")?,
    };
    Ok(
        ViewerServiceClient::with_interceptor(channel, authorization)
            .max_decoding_message_size(VIEWER_ROW_MAX_ENCODED_BYTES + 64 * 1_024),
    )
}
