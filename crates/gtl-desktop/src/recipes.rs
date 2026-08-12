//! Recipe lifecycle orchestration for the concrete desktop presentation.

use std::sync::{Arc, Mutex};

use gtl_application::{
    diffs::compute_commit_patch::{self, ComputeCommitPatch},
    history::record_render::{self, RecordRender},
    viewer::{
        ViewerTabId, ViewerTabKind, ViewerTabState,
        prepare_recipe::{self, PrepareRecipe, PrepareRecipeError, PrepareRecipeOk},
    },
};
use gtl_wire::recipes::Recipe;

use crate::{
    presentation::ViewerApp,
    session::{
        BeginCommitSelectionError, CachedView, CommitPatchTicket, ComputeTicket, PublishOutcome,
        ViewerSession,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecipeError {
    Failed(String),
    Stale,
}

impl std::fmt::Display for RecipeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(reason) => formatter.write_str(reason),
            Self::Stale => formatter.write_str("recipe computation was superseded"),
        }
    }
}

impl std::error::Error for RecipeError {}

impl From<String> for RecipeError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReservedRecipeComputation {
    pub(crate) recipe: Recipe,
    pub(crate) kind: ViewerTabKind,
    pub(crate) ticket: ComputeTicket,
}

#[derive(Debug, Clone)]
pub(crate) struct ReservedCommitPatchComputation {
    pub(crate) repo_root: std::path::PathBuf,
    pub(crate) commit: gtl_models::diffs::Commit,
    pub(crate) ticket: CommitPatchTicket,
}

#[derive(Debug, Clone)]
pub(crate) enum ViewerComputation {
    Recipe(ReservedRecipeComputation),
    CommitPatch(ReservedCommitPatchComputation),
}

impl ViewerComputation {
    pub(crate) const fn tab_id(&self) -> ViewerTabId {
        match self {
            Self::Recipe(request) => request.ticket.tab_id,
            Self::CommitPatch(request) => request.ticket.tab_id,
        }
    }
}

impl ReservedRecipeComputation {
    fn reserve_open(
        session: &Mutex<ViewerSession>,
        recipe: &Recipe,
        batch_id: String,
        kind: ViewerTabKind,
    ) -> Result<(ViewerTabId, Self), RecipeError> {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        let tab_id = session
            .open(recipe.clone(), batch_id, kind)
            .ok_or_else(|| RecipeError::Failed("viewer tab ids exhausted".into()))?;
        let ticket = session
            .begin_compute(tab_id)
            .ok_or_else(|| format!("tab {tab_id} closed before compute"))?;
        Ok((
            tab_id,
            Self {
                recipe: recipe.clone(),
                kind,
                ticket,
            },
        ))
    }

    fn reserve_refresh(
        session: &Mutex<ViewerSession>,
        tab_id: ViewerTabId,
    ) -> Result<Self, RecipeError> {
        let mut session = session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        let (recipe, kind) = {
            let tab = session
                .tab(tab_id)
                .ok_or_else(|| format!("unknown tab {tab_id}"))?;
            (tab.recipe.clone(), tab.tab.kind())
        };
        let ticket = session
            .refresh(tab_id)
            .ok_or_else(|| format!("unknown tab {tab_id}"))?;
        Ok(Self {
            recipe,
            kind,
            ticket,
        })
    }
}

#[derive(Clone)]
pub(crate) struct RecipeExecutor {
    app_state: gtl_infra::app_state::SqliteAppState,
    clock: gtl_infra::clock::SystemClock,
    git: gtl_infra::git_client::HybridGitClient,
    session: Arc<Mutex<ViewerSession>>,
    user_settings: gtl_infra::user_config::TomlSettingsStore,
}

impl RecipeExecutor {
    pub(crate) fn new(
        app_state: gtl_infra::app_state::SqliteAppState,
        clock: gtl_infra::clock::SystemClock,
        git: gtl_infra::git_client::HybridGitClient,
        session: Arc<Mutex<ViewerSession>>,
        user_settings: gtl_infra::user_config::TomlSettingsStore,
    ) -> Self {
        Self {
            app_state,
            clock,
            git,
            session,
            user_settings,
        }
    }

    pub(crate) fn compute_and_publish(
        &self,
        reserved: ReservedRecipeComputation,
    ) -> Result<(), RecipeError> {
        let ReservedRecipeComputation {
            recipe,
            kind,
            ticket,
        } = reserved;
        let prepared = match prepare_recipe::execute(
            PrepareRecipe {
                recipe: recipe.clone(),
                kind,
            },
            &self.user_settings,
            &self.git,
        ) {
            Ok(response) => response,
            Err(PrepareRecipeError::Probe(reason)) => {
                return Err(RecipeError::Failed(format!("{reason:#}")));
            }
            Err(PrepareRecipeError::Compute(reason)) => {
                publish_compute_error(&self.session, ticket, &format!("{reason:#}"))?;
                return Ok(());
            }
        };

        match prepared {
            PrepareRecipeOk::Broken { state } => {
                let mut session = self
                    .session
                    .lock()
                    .map_err(|error| RecipeError::Failed(error.to_string()))?;
                if session.set_state_if_current(ticket, state) == PublishOutcome::Stale {
                    return Err(RecipeError::Stale);
                }
            }
            PrepareRecipeOk::Skipped { .. } => {
                let mut session = self
                    .session
                    .lock()
                    .map_err(|error| RecipeError::Failed(error.to_string()))?;
                if session.close_if_current(ticket) == PublishOutcome::Stale {
                    return Err(RecipeError::Stale);
                }
            }
            PrepareRecipeOk::Publish {
                label,
                view,
                history,
            } => {
                let published = {
                    let mut session = self
                        .session
                        .lock()
                        .map_err(|error| RecipeError::Failed(error.to_string()))?;
                    session.publish_labeled_if_current(
                        ticket,
                        CachedView::new(Arc::clone(&view)),
                        label.clone(),
                    )
                };
                if published == PublishOutcome::Stale {
                    return Err(RecipeError::Stale);
                }
                record_render(&self.app_state, &self.clock, &history);
            }
        }
        Ok(())
    }

    pub(crate) fn compute_commit_patch_and_publish(
        &self,
        reserved: ReservedCommitPatchComputation,
    ) -> Result<(), RecipeError> {
        let ReservedCommitPatchComputation {
            repo_root,
            commit,
            ticket,
        } = reserved;
        let patch = match compute_commit_patch::execute(
            ComputeCommitPatch { repo_root, commit },
            &self.user_settings,
            &self.git,
        ) {
            Ok(patch) => Arc::new(patch),
            Err(error) => {
                let mut session = self
                    .session
                    .lock()
                    .map_err(|error| RecipeError::Failed(error.to_string()))?;
                return match session.set_commit_patch_error_if_current(
                    ticket,
                    "The selected commit could not be rendered. Show all changes and retry.".into(),
                ) {
                    PublishOutcome::Published => {
                        eprintln!("gtl-viewer commit patch failed: {error:#}");
                        Ok(())
                    }
                    PublishOutcome::Stale => Err(RecipeError::Stale),
                };
            }
        };
        let mut session = self
            .session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?;
        match session.publish_commit_patch_if_current(ticket, patch) {
            PublishOutcome::Published => Ok(()),
            PublishOutcome::Stale => Err(RecipeError::Stale),
        }
    }

    pub(crate) fn publish_failure(&self, ticket: ComputeTicket, reason: &str) {
        if let Err(error) = publish_compute_error(&self.session, ticket, reason)
            && error != RecipeError::Stale
        {
            eprintln!("gtl-viewer: failed to publish recipe error: {error}");
        }
    }
}

impl ViewerApp {
    pub(crate) fn open_recipe(
        &self,
        recipe: &Recipe,
        batch_id: String,
        kind: ViewerTabKind,
    ) -> Result<ViewerTabId, RecipeError> {
        let (tab_id, reserved) =
            ReservedRecipeComputation::reserve_open(&self.session, recipe, batch_id, kind)?;
        self.enqueue(reserved)?;
        Ok(tab_id)
    }

    pub(crate) fn refresh_recipe(&self, tab_id: ViewerTabId) -> Result<(), RecipeError> {
        let reserved = ReservedRecipeComputation::reserve_refresh(&self.session, tab_id)?;
        self.enqueue(reserved)?;
        Ok(())
    }

    pub(crate) fn select_commit(
        &self,
        tab_id: ViewerTabId,
        sha: &str,
    ) -> Result<(), SelectCommitError> {
        let reserved = {
            let mut session = self
                .session
                .lock()
                .map_err(|error| SelectCommitError::Failed(error.to_string()))?;
            let (ticket, repo_root, commit) = session
                .begin_commit_selection(tab_id, sha)
                .map_err(SelectCommitError::Reserve)?;
            ReservedCommitPatchComputation {
                repo_root,
                commit,
                ticket,
            }
        };
        let ticket = reserved.ticket;
        if let Err(reason) = self.enqueue_computation(ViewerComputation::CommitPatch(reserved)) {
            let mut session = self
                .session
                .lock()
                .map_err(|error| SelectCommitError::Failed(error.to_string()))?;
            if session.set_commit_patch_error_if_current(
                ticket,
                "The selected commit could not be queued. Show all changes and retry.".into(),
            ) == PublishOutcome::Stale
            {
                return Err(SelectCommitError::Reserve(
                    BeginCommitSelectionError::StaleRange,
                ));
            }
            eprintln!("gtl-viewer commit patch queue failed: {reason}");
        }
        Ok(())
    }

    fn enqueue(&self, reserved: ReservedRecipeComputation) -> Result<(), RecipeError> {
        let ticket = reserved.ticket;
        let active = self
            .session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?
            .active()
            == Some(ticket.tab_id);
        self.recipe_worker
            .submit(ViewerComputation::Recipe(reserved), active)
            .map_err(|error| {
                publish_compute_error(&self.session, ticket, &error.to_string()).unwrap_or_else(
                    |publish_error| {
                        eprintln!(
                            "gtl-viewer: failed to publish recipe queue error: {publish_error}"
                        );
                    },
                );
                RecipeError::Failed(error.to_string())
            })
    }

    fn enqueue_computation(&self, request: ViewerComputation) -> Result<(), String> {
        let active = self
            .session
            .lock()
            .map_err(|error| error.to_string())?
            .active()
            == Some(request.tab_id());
        self.recipe_worker
            .submit(request, active)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub(crate) enum SelectCommitError {
    Reserve(BeginCommitSelectionError),
    Failed(String),
}

fn record_render(
    app_state: &gtl_infra::app_state::SqliteAppState,
    clock: &impl gtl_application::ports::Clock,
    request: &RecordRender,
) {
    let mut connection = match app_state.connection_lock() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("gtl-viewer: failed to lock render history: {error:#}");
            return;
        }
    };
    if let Err(error) = record_render::execute(request, &mut connection, clock) {
        eprintln!("gtl-viewer: failed to record render history: {error:#}");
    }
}

fn publish_compute_error(
    session: &Mutex<ViewerSession>,
    ticket: ComputeTicket,
    reason: &str,
) -> Result<(), RecipeError> {
    eprintln!("gtl-viewer compute failed: {reason}");
    let mut session = session
        .lock()
        .map_err(|error| RecipeError::Failed(error.to_string()))?;
    match session.set_state_if_current(
        ticket,
        ViewerTabState::Error {
            reason: "The diff could not be rendered. Please retry.".into(),
        },
    ) {
        PublishOutcome::Published => Ok(()),
        PublishOutcome::Stale => Err(RecipeError::Stale),
    }
}
