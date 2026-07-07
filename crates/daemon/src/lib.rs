//! The resident git-tools daemon: axum on 127.0.0.1, discovered through the
//! port file under the data dir; see the walking-skeleton plan and the
//! pragmatic-backend-architecture spec.

mod endpoints;
pub mod lifecycle;
pub mod state;

use std::{
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};

use application::{
    diffs::{
        render_diff::RenderDiffHandler, render_diff_all::RenderDiffAllHandler,
        render_diff_subrepos::RenderDiffSubreposHandler, render_merge_diff::RenderMergeDiffHandler,
        render_squash_preview::RenderSquashPreviewHandler,
    },
    managed::{pull_all::PullAllHandler, push_all::PushAllHandler},
};
use infra::{
    artifact_store::StoreArtifacts, clock::SystemClock, diff_source::GitDiffSource,
    html_renderer::MaudRenderer, managed_manifest::TokioManagedManifest,
    push_ledger::NoOpPushLedger, remote_sync::TokioRemoteSync,
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    sync::watch,
};

use crate::{
    lifecycle::{ExeIdentity, PortFile},
    state::{AppState, DaemonMediator, Shared, now_ms},
};

/// Boot and serve the daemon until a shutdown signal, `POST /shutdown`, or the
/// idle timeout fires. Implements the lifecycle contract (bind, port file,
/// single-instance yield, graceful teardown).
///
/// # Errors
/// Returns an error if the data dir cannot be resolved, the socket cannot be
/// bound, or the port file cannot be written.
pub async fn run() -> anyhow::Result<()> {
    bootstrap::init_tracing();
    let store_root = gtl_platform::paths::store_root()?;
    let exe = std::env::current_exe()?;
    let identity = ExeIdentity::of(&exe)?;

    // Single instance: if a live same-identity daemon already serves this data
    // dir, quietly yield to it.
    if let Some(existing) = lifecycle::read_port_file(&store_root)
        && health_matches(existing.port, identity).await
    {
        tracing::info!(port = existing.port, "daemon already running; exiting");
        return Ok(());
    }

    let port: u16 = bootstrap::parse_env_or("GIT_TOOLS_DAEMON_PORT", 0)?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let bound = listener.local_addr()?.port();
    let pid = std::process::id();
    lifecycle::write_port_file(&store_root, PortFile { port: bound, pid })?;

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let shared = Arc::new(Shared {
        identity,
        version: env!("CARGO_PKG_VERSION"),
        pid,
        shutdown_tx,
        last_activity_ms: AtomicU64::new(now_ms()),
    });

    let idle_secs: u64 = bootstrap::parse_env_or("GIT_TOOLS_DAEMON_IDLE_SECS", 0)?;
    if idle_secs > 0 {
        spawn_idle_watch(shared.clone(), idle_secs);
    }

    let ledger = NoOpPushLedger;
    let ledger_refresh_secs: u64 = bootstrap::parse_env_or("GIT_TOOLS_LEDGER_REFRESH_SECS", 0)?;
    spawn_ledger_refresh(ledger, ledger_refresh_secs);

    let mediator = DaemonMediator {
        render_diff: RenderDiffHandler {
            source: GitDiffSource,
            store: StoreArtifacts,
            renderer: MaudRenderer,
            clock: SystemClock,
        },
        render_merge_diff: RenderMergeDiffHandler {
            source: GitDiffSource,
            store: StoreArtifacts,
            renderer: MaudRenderer,
            clock: SystemClock,
        },
        render_squash_preview: RenderSquashPreviewHandler {
            source: GitDiffSource,
            store: StoreArtifacts,
            renderer: MaudRenderer,
            clock: SystemClock,
        },
        render_diff_subrepos: RenderDiffSubreposHandler {
            source: GitDiffSource,
            store: StoreArtifacts,
            renderer: MaudRenderer,
            clock: SystemClock,
        },
        render_diff_all: RenderDiffAllHandler {
            source: GitDiffSource,
            store: StoreArtifacts,
            renderer: MaudRenderer,
            clock: SystemClock,
        },
        push_all: PushAllHandler {
            remote: TokioRemoteSync,
            manifest: TokioManagedManifest,
            ledger,
            clock: SystemClock,
        },
        pull_all: PullAllHandler {
            remote: TokioRemoteSync,
            manifest: TokioManagedManifest,
        },
    };
    let app = state::router(AppState { mediator, shared });

    tracing::info!(port = bound, "gtl-daemon listening on 127.0.0.1");
    let graceful = async move {
        let signal = bootstrap::shutdown_signal();
        tokio::select! {
            _ = signal => {}
            _ = shutdown_rx.changed() => {}
        }
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(graceful)
        .await?;
    lifecycle::remove_port_file_if_own(&store_root, pid);
    Ok(())
}

/// Whether a daemon at `port` answers `/health` with a matching exe identity.
///
/// A stale port file (no listener, or a different program) yields `false` so the
/// caller proceeds to bind its own socket.
async fn health_matches(port: u16, identity: ExeIdentity) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(1), probe_health(port)).await,
        Ok(Ok(remote)) if remote == identity
    )
}

/// Fetch the `/health` exe identity from a daemon on `127.0.0.1:port` with a
/// minimal hand-rolled HTTP GET (no client dependency).
async fn probe_health(port: u16) -> anyhow::Result<ExeIdentity> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .await?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await?;
    let text = String::from_utf8_lossy(&raw);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.trim())
        .ok_or_else(|| anyhow::anyhow!("malformed health response"))?;
    Ok(serde_json::from_str(body)?)
}

/// Spawns a ticker that calls `ledger.refresh()` every `refresh_secs` seconds.
/// `0` disables it (default) — mirrors `spawn_idle_watch`'s `> 0` gate. A no-op
/// today (the shipped `NoOpPushLedger`), but proves the daemon can host a
/// periodic job wired to the push-ledger seam.
fn spawn_ledger_refresh(ledger: impl application::ports::PushLedger + 'static, refresh_secs: u64) {
    if refresh_secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(refresh_secs));
        loop {
            ticker.tick().await;
            ledger.refresh().await;
        }
    });
}

/// Spawn a 1-second ticker that triggers shutdown once no request has arrived
/// for `idle_secs` seconds.
fn spawn_idle_watch(shared: Arc<Shared>, idle_secs: u64) {
    let idle_ms = idle_secs.saturating_mul(1000);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            if now_ms().saturating_sub(shared.last_activity_ms()) > idle_ms {
                tracing::info!(idle_secs, "idle timeout reached; shutting down");
                let _ = shared.shutdown_tx.send(true);
                return;
            }
        }
    });
}
