//! Recipe lifecycle orchestration for the concrete desktop presentation.

use std::sync::{Arc, Mutex};

use application::{
    diffs::View,
    history::record_render::{self, RecordRender},
    viewer::{
        ViewerTabId, ViewerTabKind, ViewerTabState,
        complete_recipe_computation::{
            self, CompleteRecipeComputation, CompleteRecipeComputationOk,
        },
        compute_recipe::{self, ComputeRecipe},
        probe_recipe::{self, ProbeRecipe, ProbeRecipeOutcome},
    },
};
use contracts::recipes::Recipe;

use crate::{
    presentation::ViewerApp,
    session::{CachedView, ComputeTicket, PublishOutcome, ViewerSession},
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
        let tab_id = session.open(recipe.clone(), batch_id, kind);
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
    app_state: infra::app_state::SqliteAppState,
    clock: infra::clock::SystemClock,
    git: infra::git_client::HybridGitClient,
    session: Arc<Mutex<ViewerSession>>,
    user_settings: infra::user_config::TomlSettingsStore,
}

impl RecipeExecutor {
    pub(crate) fn new(
        app_state: infra::app_state::SqliteAppState,
        clock: infra::clock::SystemClock,
        git: infra::git_client::HybridGitClient,
        session: Arc<Mutex<ViewerSession>>,
        user_settings: infra::user_config::TomlSettingsStore,
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
        let probe = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe.clone(),
                kind,
            },
            &self.git,
        )
        .map_err(|error| RecipeError::Failed(format!("{error:#}")))?;
        if let ProbeRecipeOutcome::Broken { state } = probe.outcome {
            let mut session = self
                .session
                .lock()
                .map_err(|error| RecipeError::Failed(error.to_string()))?;
            if session.set_state_if_current(ticket, state) == PublishOutcome::Stale {
                return Err(RecipeError::Stale);
            }
            return Ok(());
        }

        let view = match compute_recipe::execute(
            ComputeRecipe {
                recipe: recipe.clone(),
            },
            &self.user_settings,
            &self.git,
        ) {
            Ok(response) => response.view,
            Err(reason) => {
                publish_compute_error(&self.session, ticket, &format!("{reason:#}"))?;
                return Ok(());
            }
        };

        match complete_recipe_computation::execute(CompleteRecipeComputation {
            recipe: recipe.clone(),
            kind,
            view,
        }) {
            CompleteRecipeComputationOk::Skipped { .. } => {
                let mut session = self
                    .session
                    .lock()
                    .map_err(|error| RecipeError::Failed(error.to_string()))?;
                if session.close_if_current(ticket) == PublishOutcome::Stale {
                    return Err(RecipeError::Stale);
                }
            }
            CompleteRecipeComputationOk::Publish { label, view } => {
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
                record_render(&self.app_state, &self.clock, recipe, &view, label);
            }
        }
        Ok(())
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

    fn enqueue(&self, reserved: ReservedRecipeComputation) -> Result<(), RecipeError> {
        let ticket = reserved.ticket;
        let active = self
            .session
            .lock()
            .map_err(|error| RecipeError::Failed(error.to_string()))?
            .active()
            == Some(ticket.tab_id);
        self.recipe_worker
            .submit(reserved, active)
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
}

fn record_render(
    app_state: &impl application::ports::AppStateStore,
    clock: &impl application::ports::Clock,
    recipe: Recipe,
    view: &View,
    label: String,
) {
    if let Err(error) = record_render::execute(
        RecordRender {
            recipe,
            title: label,
            repo_name: view.repo_name.clone(),
            range_label: view.cmd.range.clone(),
        },
        app_state,
        clock,
    ) {
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
