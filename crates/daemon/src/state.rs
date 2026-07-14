//! Concrete daemon state and route table.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use axum::{
    Router,
    routing::{get, post},
};
use infra::{
    app_state::SqliteAppState, artifact_store::StoreArtifacts, clock::SystemClock,
    diff_source::GitDiffSource, html_renderer::MaudRenderer,
    managed_manifest::TokioManagedManifest, push_ledger::NoOpPushLedger,
    remote_sync::TokioRemoteSync, repo_probe::GitRepoProbe,
};
use tokio::sync::watch;

use crate::{endpoints, lifecycle::ExeIdentity};

/// Cross-cutting daemon state: identity for the handshake, the shutdown trigger, and idle state.
pub struct Shared {
    pub identity: ExeIdentity,
    pub version: &'static str,
    pub pid: u32,
    pub shutdown_tx: watch::Sender<bool>,
    pub last_activity_ms: AtomicU64,
}

impl Shared {
    /// Records that a request arrived, resetting the idle countdown.
    pub fn touch(&self) {
        self.last_activity_ms.store(now_ms(), Ordering::Relaxed);
    }

    /// Returns the last-activity instant in epoch milliseconds.
    pub fn last_activity_ms(&self) -> u64 {
        self.last_activity_ms.load(Ordering::Relaxed)
    }
}

/// Wall-clock now in epoch milliseconds, saturating on the impossible pre-epoch case.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Production adapters owned by the daemon process root.
#[derive(Clone)]
pub struct DaemonState {
    pub(crate) source: GitDiffSource,
    pub(crate) artifacts: StoreArtifacts,
    pub(crate) renderer: MaudRenderer,
    pub(crate) clock: SystemClock,
    pub(crate) remote: TokioRemoteSync,
    pub(crate) manifest: TokioManagedManifest,
    pub(crate) ledger: NoOpPushLedger,
    pub(crate) probe: GitRepoProbe,
    pub(crate) app_state: SqliteAppState,
    pub(crate) shared: Arc<Shared>,
}

impl DaemonState {
    /// Creates the production daemon state around its process-lifecycle state.
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            source: GitDiffSource,
            artifacts: StoreArtifacts,
            renderer: MaudRenderer,
            clock: SystemClock,
            remote: TokioRemoteSync,
            manifest: TokioManagedManifest,
            ledger: NoOpPushLedger,
            probe: GitRepoProbe,
            app_state: SqliteAppState,
            shared,
        }
    }
}

/// Builds the fully wired daemon router.
pub fn router(state: DaemonState) -> Router {
    Router::new()
        .route("/health", get(endpoints::health::handle))
        .route("/shutdown", post(endpoints::shutdown::handle))
        .route("/diffs/render", post(endpoints::diffs::render::handle))
        .route("/diffs/merge", post(endpoints::diffs::merge::handle))
        .route(
            "/diffs/squash-preview",
            post(endpoints::diffs::squash_preview::handle),
        )
        .route("/diffs/subrepos", post(endpoints::diffs::subrepos::handle))
        .route("/diffs/all", post(endpoints::diffs::all::handle))
        .route(
            "/managed/push-all",
            post(endpoints::managed::push_all::handle),
        )
        .route(
            "/managed/pull-all",
            post(endpoints::managed::pull_all::handle),
        )
        .route(
            "/live-views/save",
            post(endpoints::live_views::save::handle),
        )
        .with_state(state)
}
