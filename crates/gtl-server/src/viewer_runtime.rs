//! Runs viewer computations outside the session lock and publishes current results.

use gtl_application::{
    history::record_render,
    live_views::list_live_views,
    ports::Clock as _,
    recipes::RecipeBatch,
    viewer::work::{
        self, CommitPublication, RecipePublication, ReserveRecipeError, ReservedCommitWork,
        ReservedRecipeWork,
    },
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

pub(crate) fn restore_saved_live_views(state: &AppState) -> anyhow::Result<()> {
    let records = {
        let connection = state.database.connection_lock()?;
        list_live_views::execute(list_live_views::ListLiveViews, &connection)?
    };
    if let Some(work) = work::reserve_restored_live_views(&state.viewer, records)? {
        spawn_recipe(state.clone(), work);
    }
    Ok(())
}

pub(crate) fn spawn_recipe(state: AppState, work: ReservedRecipeWork) {
    tokio::task::spawn_blocking(move || {
        let work = work::compute_recipe(work, &state.user_settings, &state.git, &state.database);
        match work::publish_recipe(&state.viewer, work) {
            Ok(RecipePublication::Published { history }) => {
                record_history(&state, &history);
            }
            Ok(RecipePublication::Skipped { path }) => {
                record_project_renders(&state, &[path]);
            }
            Ok(RecipePublication::Failed { error }) => {
                tracing::error!(error = ?error, "viewer recipe computation failed");
            }
            Ok(RecipePublication::Broken | RecipePublication::Stale) => {}
            Err(error) => {
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
        let work = work::compute_commit(work, &state.user_settings, &state.git);
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

fn record_history(state: &AppState, history: &record_render::RecordRender) {
    let result = state.database.connection_lock().and_then(|mut connection| {
        record_render::execute(history, &mut connection, &state.clock).map_err(anyhow::Error::from)
    });
    if let Err(error) = result {
        tracing::error!(error = ?error, "viewer history recording failed");
    }
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
