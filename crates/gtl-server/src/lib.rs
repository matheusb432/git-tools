use std::{future::Future, time::Duration};

use anyhow::Context as _;
use gtl_local_transport::{LocalEndpoint, LocalListener};

pub mod data;
pub mod doctor;

#[cfg(any(test, feature = "benchmark-support"))]
mod harness;
mod observability;
mod server;
mod services;
mod state;
mod viewer_process;
mod viewer_runtime;

#[cfg(feature = "benchmark-support")]
pub use harness::ServerHarness;

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(10);
const LISTENER_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

pub async fn run() -> anyhow::Result<()> {
    let _observability_guard = observability::initialize()?;
    let endpoint =
        LocalEndpoint::from_environment().context("resolving the local gtl-server endpoint")?;
    let listener = LocalListener::bind(&endpoint, LISTENER_PROBE_TIMEOUT)
        .await
        .context("binding the local gRPC listener")?;
    let state = state::AppState::open(endpoint.data_root())?;
    let tab_saving =
        viewer_runtime::restore_viewer_tabs(&state).context("restoring saved viewer tabs")?;
    let shutdown = shutdown_signal()?;

    tracing::info!(native_path = %endpoint.path().display(), "gtl-server ready");
    let served = server::serve(listener, shutdown, SHUTDOWN_GRACE_PERIOD, state)
        .await
        .context("serving gtl-server");
    tab_saving.stop().await;
    served?;
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

#[cfg(windows)]
fn shutdown_signal() -> anyhow::Result<impl Future<Output = ()>> {
    use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close};

    let mut interrupt = ctrl_c().context("registering Ctrl+C handler")?;
    let mut terminate = ctrl_break().context("registering Ctrl+Break handler")?;
    let mut close = ctrl_close().context("registering console-close handler")?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
            _ = close.recv() => {}
        }
        tracing::info!("shutdown signal received");
    })
}
