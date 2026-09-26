//! Runs viewer computations outside the session lock and publishes current results.

use gtl_application::{
    history::{
        get_recent_render,
        record_render::{self, RenderErrorCode, RenderFailure, StartRender},
    },
    ports::Clock as _,
    recipes::RecipeBatch,
    viewer::{
        saved_tabs::{self, SavedViewerTab},
        work::{
            self, CommitPublication, RecipePublication, ReserveRecipeError, ReservedCommitWork,
            ReservedRecipeWork,
        },
    },
};
use gtl_models::{
    failure::ViewerFailure,
    viewer::{RenderHistoryId, ViewerTabState},
};

use crate::state::AppState;

pub(crate) fn open_recipe_batch(
    state: &AppState,
    batch: RecipeBatch,
) -> Result<(), ReserveRecipeError> {
    for item in work::reserve_recipe_batch(&state.viewer, batch)? {
        spawn_recipe(state.clone(), item);
    }
    Ok(())
}

/// Restores the saved tab strip, then keeps saving it after every tab change until stopped.
pub(crate) fn restore_viewer_tabs(state: &AppState) -> anyhow::Result<ViewerTabSaving> {
    state.database.associate_render_projects()?;
    let saved = {
        let connection = state.database.connection_lock()?;
        saved_tabs::load(&connection)
    };
    let saved = saved.unwrap_or_else(|error| {
        tracing::error!(error = ?error, "saved viewer tabs are unreadable; starting without them");
        Vec::new()
    });
    if let Some(work) = saved_tabs::restore(&state.viewer, saved.clone())? {
        spawn_recipe(state.clone(), work);
    }
    Ok(ViewerTabSaving::spawn(state.clone(), saved))
}

/// Saves the tab strip whenever a viewer change alters it; a failed save retries on the next
/// change.
pub(crate) struct ViewerTabSaving {
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl ViewerTabSaving {
    fn spawn(state: AppState, mut saved: Vec<SavedViewerTab>) -> Self {
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let mut versions = state.viewer.subscribe();
        let task = tokio::spawn(async move {
            loop {
                save_changed_tabs(&state, &mut saved).await;
                tokio::select! {
                    changed = versions.changed() => if changed.is_err() { break; },
                    _ = &mut stopped => {
                        save_changed_tabs(&state, &mut saved).await;
                        break;
                    }
                }
            }
        });
        Self { stop, task }
    }

    /// Saves any unsaved tab change, then stops saving.
    pub(crate) async fn stop(self) {
        let _ = self.stop.send(());
        if let Err(error) = self.task.await {
            tracing::error!(error = ?error, "viewer tab saving stopped abnormally");
        }
    }
}

async fn save_changed_tabs(state: &AppState, saved: &mut Vec<SavedViewerTab>) {
    let tabs = match state.viewer.inspect(|session| saved_tabs::project(session)) {
        Ok(tabs) if tabs != *saved => tabs,
        Ok(_) => return,
        Err(error) => {
            tracing::error!(error = ?error, "viewer tabs cannot be read for saving");
            return;
        }
    };
    let worker = state.clone();
    let pending = tabs.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut connection = worker.database.connection_lock()?;
        saved_tabs::save(&mut connection, &pending).map_err(anyhow::Error::from)
    })
    .await
    .map_err(anyhow::Error::from)
    .and_then(|result| result);
    match result {
        Ok(()) => *saved = tabs,
        Err(error) => tracing::error!(error = ?error, "viewer tabs could not be saved"),
    }
}

pub(crate) fn spawn_recipe(state: AppState, work: ReservedRecipeWork) {
    tokio::task::spawn_blocking(move || {
        let ticket = work.ticket();
        let render_id = start_history(&state, &work);
        let work = work::compute_recipe(
            work,
            &state.user_settings,
            &state.git,
            &state.database,
            &state.database,
        );
        match work::publish_recipe(&state.viewer, work) {
            Ok(RecipePublication::Published { history }) => {
                if let Some(render_id) = render_id {
                    succeed_history(&state, ticket, render_id, &history);
                }
            }
            Ok(RecipePublication::Skipped { path }) => {
                if let Some(render_id) = render_id {
                    discard_history(&state, render_id);
                }
                record_project_renders(&state, &[path]);
            }
            Ok(RecipePublication::Broken { state: broken }) => {
                if let Some(render_id) = render_id {
                    fail_history(&state, render_id, &broken_failure(&broken));
                }
            }
            Ok(RecipePublication::Failed { error }) => {
                if let Some(render_id) = render_id {
                    fail_history(
                        &state,
                        render_id,
                        &RenderFailure::from_error(RenderErrorCode::RenderFailed, &error),
                    );
                }
                tracing::error!(error = ?error, "viewer recipe computation failed");
            }
            Ok(RecipePublication::Stale) => {
                if let Some(render_id) = render_id {
                    discard_history(&state, render_id);
                }
            }
            Err(error) => {
                if let Some(render_id) = render_id {
                    fail_history(
                        &state,
                        render_id,
                        &RenderFailure::from_error(RenderErrorCode::PublicationFailed, &error),
                    );
                }
                tracing::error!(error = ?error, "viewer recipe publication failed");
            }
        }
    });
}

pub(crate) fn spawn_full_context(
    state: AppState,
    work: gtl_application::viewer::ensure_view_full_context::ReservedFullContext,
) {
    tokio::task::spawn_blocking(move || {
        if let Err(error) = gtl_application::viewer::ensure_view_full_context::execute_reserved(
            work,
            &state.viewer,
            &state.git,
        ) {
            tracing::error!(error = ?error, "viewer full-context source preparation failed");
        }
    });
}

pub(crate) fn spawn_commit(state: AppState, work: ReservedCommitWork) {
    tokio::task::spawn_blocking(move || {
        let work = work::compute_commit(work, &state.user_settings, &state.git, &state.database);
        match work::publish_commit(&state.viewer, work) {
            Ok(CommitPublication::Failed { error }) => {
                tracing::error!(error = ?error, "viewer commit computation failed");
            }
            Ok(CommitPublication::Published | CommitPublication::Stale) => {}
            Err(error) => {
                tracing::error!(error = ?error, "viewer commit publication failed");
            }
        }
    });
}

fn start_history(state: &AppState, work: &ReservedRecipeWork) -> Option<RenderHistoryId> {
    let result = state.database.connection_lock().and_then(|mut connection| {
        record_render::start(
            &StartRender::new(work.recipe().clone()),
            &mut connection,
            &state.clock,
        )
        .map_err(anyhow::Error::from)
    });
    match result {
        Ok(render_id) => Some(render_id),
        Err(error) => {
            tracing::error!(error = ?error, "viewer pending history recording failed");
            None
        }
    }
}

fn succeed_history(
    state: &AppState,
    ticket: gtl_application::viewer::session::ComputeTicket,
    render_id: RenderHistoryId,
    history: &record_render::RecordRender,
) {
    let result = state.database.connection_lock().and_then(|mut connection| {
        let id = record_render::succeed(render_id, history, &mut connection)?;
        let record =
            get_recent_render::execute(&get_recent_render::GetRecentRender { id }, &connection)?
                .ok_or_else(|| anyhow::anyhow!("completed render is unavailable"))?;
        state
            .viewer
            .update(|session| session.bind_snapshot_history(ticket, &record))?;
        Ok(())
    });
    let result = result.and_then(|()| state.database.associate_render_projects());
    if let Err(error) = result {
        tracing::error!(error = ?error, "viewer successful history publication failed");
    }
}

fn fail_history(state: &AppState, render_id: RenderHistoryId, failure: &RenderFailure) {
    let result = state.database.connection_lock().and_then(|mut connection| {
        record_render::fail(render_id, failure, &mut connection).map_err(anyhow::Error::from)
    });
    if let Err(error) = result {
        tracing::error!(error = ?error, "viewer failure history recording failed");
    }
}

fn discard_history(state: &AppState, render_id: RenderHistoryId) {
    let result = state.database.connection_lock().and_then(|mut connection| {
        record_render::discard(render_id, &mut connection).map_err(anyhow::Error::from)
    });
    if let Err(error) = result {
        tracing::error!(error = ?error, "viewer pending history discard failed");
    }
}

fn broken_failure(state: &ViewerTabState) -> RenderFailure {
    let ViewerTabState::Broken { failure } = state else {
        return RenderFailure::new(
            RenderErrorCode::SourceUnavailable,
            format!("unexpected broken render state: {state:?}"),
        );
    };
    let code = match failure {
        ViewerFailure::SourceDirectoryMissing { .. } => {
            RenderErrorCode::RepositoryDirectoryNotFound
        }
        ViewerFailure::SourceNotRepository { .. } => {
            RenderErrorCode::RepositoryDirectoryNotGitRepository
        }
        _ => RenderErrorCode::SourceUnavailable,
    };
    RenderFailure::new(code, failure.to_string())
}

pub(crate) fn record_project_renders(
    state: &AppState,
    paths: &[gtl_models::paths::RepositoryRoot],
) {
    use gtl_application::projects::record_project_render::{self, RecordProjectRender};
    let result = (|| -> anyhow::Result<()> {
        let mut connection = state.database.connection_lock()?;
        let transaction = connection.transaction()?;
        let rendered_at = state.clock.now()?;
        for path in paths {
            record_project_render::execute(
                &RecordProjectRender {
                    path: path.clone(),
                    rendered_at: rendered_at.clone(),
                },
                &transaction,
            )?;
        }
        transaction.commit()?;
        Ok(())
    })();
    if let Err(error) = result {
        tracing::error!(error = ?error, "project render recency recording failed");
    }
}
