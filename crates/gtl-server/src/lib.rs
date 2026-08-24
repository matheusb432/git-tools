use std::{future::Future, time::Duration};

use anyhow::Context as _;
use gtl_local_auth::{
    CapabilityToken, LocalAuth, ServerEndpoint, ServerInstanceId, ViewerBootstrap,
};
use gtl_wire::viewer::VIEWER_PROTOCOL_VERSION;

mod config;
#[cfg(any(test, feature = "benchmark-support"))]
mod harness;
mod observability;
mod server;
mod services;
mod state;
mod viewer_process;
mod viewer_runtime;

use config::Config;
#[cfg(feature = "benchmark-support")]
pub use harness::ServerHarness;

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(10);

pub async fn run() -> anyhow::Result<()> {
    let _observability_guard = observability::initialize()?;
    let config = Config::from_env()?;
    let local_auth = LocalAuth::from_environment()?;
    let capability = local_auth.load_or_create_server_token()?;
    let state = state::AppState::open(local_auth.data_root())?;
    viewer_runtime::restore_saved_live_views(&state)
        .context("restoring saved live views into the server viewer session")?;
    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .with_context(|| format!("binding gtl-server to {}", config.bind_address))?;
    let local_address = listener
        .local_addr()
        .context("reading the gtl-server listen address")?;
    let endpoint = ServerEndpoint::try_new(local_address, ServerInstanceId::generate())?;
    let viewer_capability = CapabilityToken::generate()?;
    let _published_endpoint = local_auth.publish_endpoint(endpoint.clone())?;
    let _published_viewer = local_auth.publish_viewer_bootstrap(&ViewerBootstrap::new(
        endpoint,
        viewer_capability.clone(),
        VIEWER_PROTOCOL_VERSION,
    ))?;
    let shutdown = shutdown_signal()?;

    tracing::info!(address = %local_address, "gtl-server ready");
    server::serve(
        listener,
        shutdown,
        SHUTDOWN_GRACE_PERIOD,
        capability,
        viewer_capability,
        state,
    )
    .await
    .context("serving gtl-server")?;
    tracing::info!("gtl-server stopped");
    Ok(())
}

#[cfg(unix)]
fn shutdown_signal() -> anyhow::Result<impl Future<Output = ()>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate()).context("registering SIGTERM handler")?;
    Ok(async move {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                if let Err(error) = result {
                    tracing::error!(error = ?error, "SIGINT handler failed");
                }
            }
            _ = terminate.recv() => {}
        }
        tracing::info!("shutdown signal received");
    })
}

#[cfg(not(unix))]
fn shutdown_signal() -> anyhow::Result<impl Future<Output = ()>> {
    Ok(async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(error = ?error, "interrupt handler failed");
        }
        tracing::info!("shutdown signal received");
    })
}
