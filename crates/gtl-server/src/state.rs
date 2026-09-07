use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::Context as _;
use gtl_artifacts::ArtifactRenderer;
use gtl_infra::{
    app_state::SqliteAppState, artifact_store::StoreArtifacts, clock::SystemClock,
    file_system::LocalFileSystemClient, git_client::HybridGitClient,
    project_repository_client::ProjectRepositoryClient, text_editor::GitTextEditorClient,
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
    pub(crate) file_system: LocalFileSystemClient,
    pub(crate) text_editor: GitTextEditorClient,
    pub(crate) viewer: gtl_application::viewer::ViewerState,
    pub(crate) viewer_row_streams: ViewerRowStreams,
    pub(crate) live_refresh_permits: Arc<tokio::sync::Semaphore>,
}

impl AppState {
    pub(crate) fn open(data_root: &Path) -> anyhow::Result<Self> {
        Self::open_with_settings(data_root, TomlSettingsStore::from_environment())
    }

    pub(crate) fn open_with_settings(
        data_root: &Path,
        user_settings: TomlSettingsStore,
    ) -> anyhow::Result<Self> {
        let database = SqliteAppState::open(data_root)
            .with_context(|| format!("opening application state at {}", data_root.display()))?;
        Ok(Self {
            git: HybridGitClient,
            artifacts: StoreArtifacts,
            renderer: ArtifactRenderer,
            clock: SystemClock,
            projects: ProjectRepositoryClient::new(database.clone()),
            database,
            user_settings,
            file_system: LocalFileSystemClient,
            text_editor: GitTextEditorClient,
            viewer: gtl_application::viewer::ViewerState::new(),
            viewer_row_streams: ViewerRowStreams::default(),
            live_refresh_permits: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
}

#[derive(Clone, Default)]
pub(crate) struct ViewerRowStreams {
    version: Arc<AtomicU64>,
}

impl ViewerRowStreams {
    pub(crate) fn start_stream(&self) -> Result<u64, ViewerRowStreamVersionExhausted> {
        let previous = self
            .version
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |version| {
                version.checked_add(1)
            })
            .map_err(|_| ViewerRowStreamVersionExhausted)?;
        previous
            .checked_add(1)
            .ok_or(ViewerRowStreamVersionExhausted)
    }

    pub(crate) fn is_current(&self, stream: u64) -> bool {
        self.version.load(Ordering::Acquire) == stream
    }

    pub(crate) fn cancel_current_stream(&self) -> Result<(), ViewerRowStreamVersionExhausted> {
        self.start_stream().map(|_| ())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("viewer row stream version is exhausted")]
pub(crate) struct ViewerRowStreamVersionExhausted;

#[cfg(test)]
mod tests {
    use super::ViewerRowStreams;

    #[test]
    fn starting_a_row_stream_replaces_the_previous_stream() {
        let streams = ViewerRowStreams::default();
        let first = streams.start_stream().unwrap();
        let second = streams.start_stream().unwrap();

        assert!(!streams.is_current(first));
        assert!(streams.is_current(second));
    }

    #[test]
    fn settings_change_cancels_the_current_row_stream() {
        let streams = ViewerRowStreams::default();
        let stream = streams.start_stream().unwrap();

        streams.cancel_current_stream().unwrap();

        assert!(!streams.is_current(stream));
    }
}
