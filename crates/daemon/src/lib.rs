//! The resident git-tools daemon: axum on 127.0.0.1, discovered through the
//! port file under the data dir; see the walking-skeleton plan and the
//! pragmatic-backend-architecture spec.

mod endpoints;
pub mod lifecycle;
mod startup;
pub mod state;

use std::time::Duration;

use tokio::sync::watch;

use crate::{
    lifecycle::{ExeIdentity, PortFile},
    state::DaemonState,
};

/// Boot and serve the daemon until a shutdown signal or `POST /shutdown`.
/// Implements the lifecycle contract (lock, bind, port file, graceful teardown).
///
/// # Errors
/// Returns an error if the data dir cannot be resolved, the socket cannot be
/// bound, the app-state database cannot initialize, or the port file cannot be written.
pub async fn run() -> anyhow::Result<()> {
    startup::init_tracing();
    let store_root = infra::data_root::resolve()?;
    let Some(daemon_lock) =
        lifecycle::acquire_daemon_lock(&store_root, Duration::from_millis(500))?
    else {
        tracing::info!("daemon ownership is already held; exiting");
        return Ok(());
    };
    let app_state = infra::app_state::SqliteAppState::open(&store_root)?;
    let exe = std::env::current_exe()?;
    let identity = ExeIdentity::of(&exe)?;

    let port = startup::daemon_port()?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let bound = listener.local_addr()?.port();
    let pid = std::process::id();
    lifecycle::write_port_file(&store_root, PortFile { port: bound, pid })?;

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let app = state::router(DaemonState::new(
        identity,
        env!("CARGO_PKG_VERSION"),
        pid,
        shutdown_tx,
        app_state,
        infra::user_config::TomlSettingsStore::from_environment(),
    ));

    tracing::info!(port = bound, "gtl-daemon listening on 127.0.0.1");
    let graceful = async move {
        let signal = startup::shutdown_signal();
        tokio::select! {
            _ = signal => {}
            _ = shutdown_rx.changed() => {}
        }
    };
    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(graceful)
        .await;
    lifecycle::remove_port_file_if_own(&store_root, pid);
    drop(daemon_lock);
    serve_result?;
    Ok(())
}
