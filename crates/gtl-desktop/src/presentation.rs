mod restoration;

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use gtl_contracts::viewer::ViewerStateChanged;
use gtl_infra::{
    app_state::SqliteAppState, clock::SystemClock, configured_editor::GitConfiguredEditorClient,
    file_system::LocalFileSystemClient, git_client::HybridGitClient,
    user_config::TomlSettingsStore,
};
pub(crate) use restoration::RestorationGate;

use crate::{
    recipe_worker::RecipeWorker,
    recipes::RecipeExecutor,
    session::{PendingRecipes, ViewerSession},
};

#[derive(Clone)]
pub(crate) struct ViewerApp {
    pub(crate) app_state: SqliteAppState,
    pub(crate) file_system: LocalFileSystemClient,
    pub(crate) configured_editor: GitConfiguredEditorClient,
    pub(crate) session: Arc<Mutex<ViewerSession>>,
    pub(crate) recipe_worker: RecipeWorker,
    pending: Arc<PendingRecipes>,
    pub(crate) user_settings: Arc<Mutex<TomlSettingsStore>>,
    pub(crate) restoration: Arc<RestorationGate>,
}

impl ViewerApp {
    pub(crate) fn open(
        data_root: &Path,
        user_settings: TomlSettingsStore,
        max_cache_weight: usize,
    ) -> anyhow::Result<Self> {
        let app_state = SqliteAppState::open(data_root)?;
        let clock = SystemClock;
        let git = HybridGitClient;
        let session = Arc::new(Mutex::new(ViewerSession::new(max_cache_weight)));
        let recipe_worker = RecipeWorker::start(RecipeExecutor::new(
            app_state.clone(),
            clock,
            git,
            Arc::clone(&session),
            user_settings.clone(),
        ))
        .map_err(anyhow::Error::msg)?;
        Ok(Self {
            app_state,
            file_system: LocalFileSystemClient,
            configured_editor: GitConfiguredEditorClient,
            session,
            recipe_worker,
            pending: Arc::new(PendingRecipes::default()),
            user_settings: Arc::new(Mutex::new(user_settings)),
            restoration: Arc::new(RestorationGate::default()),
        })
    }

    pub(crate) fn pending(&self) -> &PendingRecipes {
        &self.pending
    }

    pub(crate) fn take_recipe_completions(&self) -> Option<std::sync::mpsc::Receiver<()>> {
        self.recipe_worker.take_completions()
    }

    pub(crate) fn state_changed(&self) -> Result<ViewerStateChanged, String> {
        let revision = self
            .session
            .lock()
            .map_err(|error| format!("failed to lock viewer session: {error}"))?
            .revision();
        Ok(ViewerStateChanged { revision })
    }
}
