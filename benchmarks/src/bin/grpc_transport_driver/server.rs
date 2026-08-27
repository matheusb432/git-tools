use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use gtl_benchmarks::{
    grpc_transport::TransportKind,
    release_server::{ReleaseServerConfig, ReleaseServerProcess},
};
use gtl_client::GtlClient;
use gtl_local_auth::{CapabilityToken, LocalAuth, ServerEndpoint};

use super::environment::DriverConfig;

const READY_TIMEOUT: Duration = Duration::from_secs(60);
const READY_RETRY_DELAY: Duration = Duration::from_millis(10);
const SETTINGS: &str = "[push]\nconfirm = true\n";

pub struct ServerAccess {
    pub target: String,
    pub transport: TransportKind,
    pub capability: CapabilityToken,
    pub push_confirmation_required: bool,
}

pub struct BenchmarkServer {
    process: ReleaseServerProcess,
}

impl BenchmarkServer {
    pub fn start(config: &DriverConfig, root: &std::path::Path) -> Result<Self> {
        Ok(Self {
            process: ReleaseServerProcess::start(
                ReleaseServerConfig {
                    server_binary: &config.server_binary,
                    settings: SETTINGS,
                    bind_address: None,
                    profiler: None,
                },
                root,
            )?,
        })
    }

    pub fn process_id(&self) -> u32 {
        self.process.process_id()
    }

    pub async fn wait_until_ready(&mut self) -> Result<ServerAccess> {
        let auth = LocalAuth::from_data_root(self.process.data_root())
            .context("open isolated server authentication")?;
        let started = Instant::now();
        loop {
            self.process.ensure_running()?;
            let last_error = match GtlClient::connect(&auth).await {
                Ok(client) => match client.get_push_confirmation_requirement().await {
                    Ok(response) => {
                        if !response.push_confirmation_required {
                            bail!("isolated server ignored the fixed push confirmation setting");
                        }
                        let endpoint = auth
                            .load_endpoint()
                            .context("load ready benchmark endpoint")?;
                        let (target, transport) = benchmark_target(&endpoint);
                        let capability = auth
                            .load_client_token()
                            .context("load benchmark capability")?;
                        return Ok(ServerAccess {
                            target,
                            transport,
                            capability,
                            push_confirmation_required: response.push_confirmation_required,
                        });
                    }
                    Err(error) => format!("settings request failed: {error}"),
                },
                Err(error) => format!("connection failed: {error}"),
            };
            if started.elapsed() >= READY_TIMEOUT {
                bail!(
                    "isolated release server was not ready within {} seconds: {last_error}",
                    READY_TIMEOUT.as_secs()
                );
            }
            tokio::time::sleep(READY_RETRY_DELAY).await;
        }
    }

    pub fn ensure_running(&mut self) -> Result<()> {
        self.process.ensure_running()
    }

    pub fn stop(self) -> Result<()> {
        self.process.stop()
    }
}

fn benchmark_target(endpoint: &ServerEndpoint) -> (String, TransportKind) {
    (endpoint.address().to_string(), TransportKind::Tcp)
}
