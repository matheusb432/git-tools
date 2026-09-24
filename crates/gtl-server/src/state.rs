use std::{
    path::Path,
    sync::{Arc, Mutex},
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
    pub(crate) viewer_push_operations: Arc<gtl_application::viewer::push::ViewerPushOperations>,
    pub(crate) viewer_push_requests: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_push_workers: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_file_filter_requests: Arc<tokio::sync::Semaphore>,
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
    pub(crate) viewer_row_streams: ViewerWorkRequests,
    pub(crate) viewer_row_sessions: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_row_workers: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_searches: ViewerWorkRequests,
    pub(crate) viewer_project_status_requests: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_project_status_workers: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_project_index_requests: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_project_status_checks: Arc<std::sync::atomic::AtomicU64>,
    pub(crate) viewer_project_watch_registrations: Arc<std::sync::atomic::AtomicUsize>,
    pub(crate) viewer_project_watch_requests: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_project_watch_workers: Arc<tokio::sync::Semaphore>,
    pub(crate) viewer_project_status_cache:
        Arc<Mutex<gtl_application::projects::status_cache::ProjectStatusCache>>,
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
            viewer_push_operations: Arc::default(),
            viewer_push_requests: Arc::new(tokio::sync::Semaphore::new(4)),
            viewer_push_workers: Arc::new(tokio::sync::Semaphore::new(4)),
            viewer_file_filter_requests: Arc::new(tokio::sync::Semaphore::new(1)),
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
            viewer_row_streams: ViewerWorkRequests::default(),
            viewer_row_workers: Arc::new(tokio::sync::Semaphore::new(
                gtl_wire::viewer::VIEWER_ROW_SESSIONS_MAX,
            )),
            viewer_row_sessions: Arc::new(tokio::sync::Semaphore::new(
                gtl_wire::viewer::VIEWER_ROW_SESSIONS_MAX,
            )),
            viewer_searches: ViewerWorkRequests::default(),
            viewer_project_status_requests: Arc::new(tokio::sync::Semaphore::new(100)),
            viewer_project_status_workers: Arc::new(tokio::sync::Semaphore::new(4)),
            viewer_project_index_requests: Arc::new(tokio::sync::Semaphore::new(1)),
            viewer_project_status_checks: Arc::default(),
            viewer_project_watch_registrations: Arc::default(),
            viewer_project_watch_requests: Arc::new(tokio::sync::Semaphore::new(2)),
            viewer_project_watch_workers: Arc::new(tokio::sync::Semaphore::new(1)),
            viewer_project_status_cache: Arc::default(),
            live_refresh_permits: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }
}

#[derive(Clone, Default)]
pub(crate) struct ViewerWorkRequests {
    current: Arc<Mutex<Option<gtl_application::viewer::rows::ViewerWorkCancellation>>>,
}

impl ViewerWorkRequests {
    pub(crate) fn current_stream(
        &self,
    ) -> Result<gtl_application::viewer::rows::ViewerWorkCancellation, ViewerWorkStateError> {
        Ok(self
            .current
            .lock()
            .map_err(|_| ViewerWorkStateError)?
            .get_or_insert_default()
            .clone())
    }

    pub(crate) fn start_stream(
        &self,
    ) -> Result<gtl_application::viewer::rows::ViewerWorkCancellation, ViewerWorkStateError> {
        let cancellation = gtl_application::viewer::rows::ViewerWorkCancellation::default();
        let previous = self
            .current
            .lock()
            .map_err(|_| ViewerWorkStateError)?
            .replace(cancellation.clone());
        if let Some(previous) = previous {
            previous.cancel();
        }
        Ok(cancellation)
    }

    pub(crate) fn cancel_current_stream(&self) -> Result<(), ViewerWorkStateError> {
        let previous = self
            .current
            .lock()
            .map_err(|_| ViewerWorkStateError)?
            .take();
        if let Some(previous) = previous {
            previous.cancel();
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("viewer work state lock is poisoned")]
pub(crate) struct ViewerWorkStateError;

impl gtl_models::failure::Classified for ViewerWorkStateError {
    fn classify(&self) -> gtl_models::failure::Classification {
        gtl_models::failure::Classification::Private(gtl_models::failure::ErrorClass::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::ViewerWorkRequests;

    #[test]
    fn starting_a_row_stream_cancels_the_previous_worker() {
        let streams = ViewerWorkRequests::default();
        let first = streams.start_stream().unwrap();
        let second = streams.start_stream().unwrap();
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
    }

    #[test]
    fn settings_change_cancels_the_current_row_worker() {
        let streams = ViewerWorkRequests::default();
        let stream = streams.start_stream().unwrap();
        streams.cancel_current_stream().unwrap();
        assert!(stream.is_cancelled());
    }
}
