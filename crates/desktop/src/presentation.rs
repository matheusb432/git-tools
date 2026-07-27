mod restoration;

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use infra::{
    app_state::SqliteAppState, clock::SystemClock, configured_editor::GitConfiguredEditorClient,
    diff_source::GitDiffSource, file_system::LocalFileSystemClient, repo_probe::GitRepoProbe,
    user_config::TomlSettingsStore,
};
pub(crate) use restoration::RestorationGate;

use crate::{
    render::MaudViewerRenderer,
    session::{PendingRecipes, ViewerSession},
};

#[derive(Clone)]
pub(crate) struct ViewerApp {
    pub(crate) clock: SystemClock,
    pub(crate) probe: GitRepoProbe,
    pub(crate) app_state: SqliteAppState,
    pub(crate) source: GitDiffSource,
    pub(crate) file_system: LocalFileSystemClient,
    pub(crate) configured_editor: GitConfiguredEditorClient,
    pub(crate) session: Arc<Mutex<ViewerSession>>,
    pending: Arc<PendingRecipes>,
    pub(crate) renderer: MaudViewerRenderer,
    pub(crate) user_settings: TomlSettingsStore,
    pub(crate) restoration: Arc<RestorationGate>,
}

impl ViewerApp {
    pub(crate) fn open(
        data_root: &Path,
        user_settings: TomlSettingsStore,
        max_cache_weight: usize,
    ) -> anyhow::Result<Self> {
        let app_state = SqliteAppState::open(data_root)?;
        Ok(Self {
            clock: SystemClock,
            probe: GitRepoProbe,
            app_state,
            source: GitDiffSource,
            file_system: LocalFileSystemClient,
            configured_editor: GitConfiguredEditorClient,
            session: Arc::new(Mutex::new(ViewerSession::new(max_cache_weight))),
            pending: Arc::new(PendingRecipes::default()),
            renderer: MaudViewerRenderer,
            user_settings,
            restoration: Arc::new(RestorationGate::default()),
        })
    }

    pub(crate) fn pending(&self) -> &PendingRecipes {
        &self.pending
    }
}
