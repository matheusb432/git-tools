//! Concrete daemon state and route table.

use axum::{
    Router,
    routing::{get, post},
};
use gtl_artifacts::ArtifactRenderer;
use gtl_infra::{
    app_state::SqliteAppState, sample_project_project_client::SampleProjectClient,
    artifact_store::StoreArtifacts, clock::SystemClock, git_client::HybridGitClient,
    push_ledger::NoOpPushLedger, user_config::TomlSettingsStore,
};
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
    pub(crate) renderer: ArtifactRenderer,
    pub(crate) clock: SystemClock,
    pub(crate) projects: SampleProjectClient,
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
            renderer: ArtifactRenderer,
            clock: SystemClock,
            projects: SampleProjectClient::from_environment(),
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
        .route("/tags/bump/dry-run", post(endpoints::tags::dry_run::handle))
        .route("/tags/bump", post(endpoints::tags::bump::handle))
        .with_state(state)
}
