use gtl_application::{
    live_views,
    viewer::{ViewerTabId, ViewerTabKind},
};
use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};
use tauri::http::StatusCode;

use super::{
    response::{RouteError, RouteOutput, RouteResult},
    settings, tabs, view_loading,
    view_snapshot::VersionedView,
};
use crate::{presentation::ViewerApp, render::SwapFeedback};

pub(super) fn delete(app: &ViewerApp, tab: ViewerTabId) -> RouteResult {
    let source = {
        let session = app.session.lock().map_err(|error| error.to_string())?;
        session.live_source(tab)
    };
    let Some(source) = source else {
        return Ok(RouteOutput::Empty(StatusCode::NOT_FOUND));
    };

    {
        let connection = app
            .app_state
            .connection_lock()
            .map_err(|error| format!("{error:#}"))?;
        live_views::remove::execute(
            live_views::remove::RemoveLiveView {
                source_kind: source.kind().into(),
                source_value: source.value(),
            },
            &connection,
        )
        .map_err(|error| format!("{error:#}"))?;
    }

    let closed = app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .close_live_view(tab, &source);
    if !closed {
        return Err(RouteError::Conflict);
    }

    let transient = view_loading::ensure_active_view(app)?;
    let settings = settings::load(app);
    let load_id = view_loading::prepare_materialization(app, settings.options())?;
    tabs::render_tabs_with_view(
        app.renderer,
        &app.session,
        transient,
        settings,
        SwapFeedback::LiveViewDeleted,
        load_id,
    )
    .map(RouteOutput::Html)
    .map_err(Into::into)
}

pub(super) fn restore(app: &ViewerApp) -> Result<Option<VersionedView>, RouteError> {
    let owner = app.restoration.run_once(|| {
        let records = {
            let connection = app
                .app_state
                .connection_lock()
                .map_err(|error| format!("{error:#}"))?;
            live_views::list::execute(live_views::list::ListLiveViews, &connection)
                .map_err(|error| format!("{error:#}"))?
                .views
        };
        let mut newest = None;
        {
            let mut session = app.session.lock().map_err(|error| error.to_string())?;
            for record in records {
                if record.source_kind != "LocalRepo" {
                    continue;
                }
                let recipe = Recipe {
                    source: RecipeSource::LocalRepo(record.source_value.into()),
                    op: RecipeOp::Diff {
                        target: RecipeTarget::Unpushed { pinned: None },
                    },
                    name: Some(record.display_name),
                };
                newest = Some(session.open(recipe, "restored-live".into(), ViewerTabKind::Live));
            }
        }
        if let Some(tab) = newest {
            app.refresh_recipe(tab).map_err(|error| error.to_string())?;
        }
        Ok(())
    })?;
    if owner {
        Ok(None)
    } else {
        view_loading::ensure_active_view(app)
    }
}
