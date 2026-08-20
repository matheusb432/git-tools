use std::path::Path;

use anyhow::Context as _;
use gtl_artifacts::ArtifactRenderer;
use gtl_infra::{
    app_state::SqliteAppState, artifact_store::StoreArtifacts, clock::SystemClock,
    git_client::HybridGitClient, project_repository_client::ProjectRepositoryClient,
    user_config::TomlSettingsStore,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) git: HybridGitClient,
    pub(crate) artifacts: StoreArtifacts,
    pub(crate) renderer: ArtifactRenderer,
    pub(crate) clock: SystemClock,
    pub(crate) projects: ProjectRepositoryClient,
    pub(crate) database: SqliteAppState,
    pub(crate) user_settings: TomlSettingsStore,
}

impl AppState {
    pub(crate) fn open(data_root: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            git: HybridGitClient,
            artifacts: StoreArtifacts,
            renderer: ArtifactRenderer,
            clock: SystemClock,
            projects: ProjectRepositoryClient::from_environment(),
            database: SqliteAppState::open(data_root)
                .with_context(|| format!("opening application state at {}", data_root.display()))?,
            user_settings: TomlSettingsStore::from_environment(),
        })
    }
}
