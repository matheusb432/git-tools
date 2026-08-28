use std::{future::Future, time::Duration};

use anyhow::Context as _;
#[cfg(windows)]
use gtl_local_auth::ServerEndpoint;
use gtl_local_auth::{LocalAuth, ServerInstanceId, ViewerBootstrap};
use gtl_wire::viewer::VIEWER_PROTOCOL_VERSION;

#[cfg(any(test, feature = "benchmark-support"))]
mod harness;
mod observability;
mod server;
mod services;
mod state;
#[cfg(unix)]
mod uds_listener;
mod viewer_process;
mod viewer_runtime;

#[cfg(feature = "benchmark-support")]
pub use harness::ServerHarness;

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(10);
#[cfg(windows)]
const LOOPBACK_EPHEMERAL_ADDRESS: &str = "127.0.0.1:0";

pub async fn run() -> anyhow::Result<()> {
    let _observability_guard = observability::initialize()?;
    let local_auth = LocalAuth::from_environment()?;
    let capability = local_auth.load_or_create_server_token()?;
    let state = state::AppState::open(local_auth.data_root())?;
    viewer_runtime::restore_saved_live_views(&state)
        .context("restoring saved live views into the server viewer session")?;
    let instance_id = ServerInstanceId::generate();
    #[cfg(unix)]
    let (endpoint, listeners) = {
        let endpoint = local_auth.server_endpoint(instance_id.clone())?;
        let native_listener = uds_listener::BoundUdsListener::bind(endpoint.uds_path())?;
        let listeners = server::ServerListeners::new(native_listener);
        (endpoint, listeners)
    };
    #[cfg(windows)]
    let (endpoint, listeners) = {
        let listener = tokio::net::TcpListener::bind(LOOPBACK_EPHEMERAL_ADDRESS)
            .await
            .context("binding gtl-server to loopback")?;
        let address = listener
            .local_addr()
            .context("reading the gtl-server listen address")?;
        let endpoint = ServerEndpoint::try_new(address, instance_id.clone())?;
        let listeners = server::ServerListeners::new(listener);
        (endpoint, listeners)
    };
    let _published_viewer = local_auth
        .publish_viewer_bootstrap(&ViewerBootstrap::new(instance_id, VIEWER_PROTOCOL_VERSION))?;
    let _published_endpoint = local_auth.publish_endpoint(endpoint.clone())?;
    let shutdown = shutdown_signal()?;

    #[cfg(unix)]
    tracing::info!(native_path = %endpoint.uds_path().display(), "gtl-server ready");
    #[cfg(windows)]
    tracing::info!(address = %endpoint.tcp_address(), "gtl-server ready");
    server::serve(
        listeners,
        shutdown,
        SHUTDOWN_GRACE_PERIOD,
        capability,
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
