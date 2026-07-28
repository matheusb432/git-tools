//! Concrete daemon state and route table.

use axum::{
    Router,
    routing::{get, post},
};
use infra::{
    app_state::SqliteAppState, artifact_store::StoreArtifacts, clock::SystemClock,
    git_client::HybridGitClient, managed_manifest::TokioManagedManifest,
    push_ledger::NoOpPushLedger, user_config::TomlSettingsStore,
};
use preview::MaudRenderer;
use tokio::sync::watch;

use crate::{endpoints, lifecycle::ExeIdentity};

/// Production adapters owned by the daemon process root.
#[derive(Clone)]
pub struct DaemonState {
    pub(crate) identity: ExeIdentity,
    pub(crate) version: &'static str,
    pub(crate) pid: u32,
    pub(crate) shutdown_tx: watch::Sender<bool>,
    pub(crate) git: HybridGitClient,
    pub(crate) artifacts: StoreArtifacts,
    pub(crate) renderer: MaudRenderer,
    pub(crate) clock: SystemClock,
    pub(crate) manifest: TokioManagedManifest,
    pub(crate) ledger: NoOpPushLedger,
    pub(crate) app_state: SqliteAppState,
    pub(crate) user_settings: TomlSettingsStore,
}

impl DaemonState {
    /// Creates the daemon state from startup identity and concrete adapters.
    pub fn new(
        identity: ExeIdentity,
        version: &'static str,
        pid: u32,
        shutdown_tx: watch::Sender<bool>,
        app_state: SqliteAppState,
        user_settings: TomlSettingsStore,
    ) -> Self {
        Self {
            identity,
            version,
            pid,
            shutdown_tx,
            git: HybridGitClient,
            artifacts: StoreArtifacts,
            renderer: MaudRenderer,
            clock: SystemClock,
            manifest: TokioManagedManifest,
            ledger: NoOpPushLedger,
            app_state,
            user_settings,
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
