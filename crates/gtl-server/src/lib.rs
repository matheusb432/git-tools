use std::{future::Future, time::Duration};

use anyhow::Context as _;
use gtl_local_auth::{LocalAuth, ServerEndpoint, ServerInstanceId};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

mod config;
mod server;

use config::Config;

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(10);

pub async fn run() -> anyhow::Result<()> {
    initialize_tracing()?;
    let config = Config::from_env()?;
    let local_auth = LocalAuth::from_environment()?;
    let capability = local_auth.load_or_create_server_token()?;
    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .with_context(|| format!("binding gtl-server to {}", config.bind_address))?;
    let local_address = listener
        .local_addr()
        .context("reading the gtl-server listen address")?;
    let endpoint = ServerEndpoint::try_new(local_address, ServerInstanceId::generate())?;
    let _published_endpoint = local_auth.publish_endpoint(endpoint)?;
    let shutdown = shutdown_signal()?;

    tracing::info!(address = %local_address, "gtl-server ready");
    server::serve(listener, shutdown, SHUTDOWN_GRACE_PERIOD, capability)
        .await
        .context("serving gtl-server")?;
    tracing::info!("gtl-server stopped");
    Ok(())
}

fn initialize_tracing() -> anyhow::Result<()> {
    let filter = if std::env::var_os("RUST_LOG").is_some() {
        EnvFilter::try_from_default_env().context("parsing RUST_LOG")?
    } else {
        EnvFilter::new("info")
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .try_init()
        .context("initializing tracing")
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
