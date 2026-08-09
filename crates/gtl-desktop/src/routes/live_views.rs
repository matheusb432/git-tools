use gtl_application::{live_views, viewer::ViewerTabId};
use tauri::http::StatusCode;

use super::{
    response::{RouteError, RouteOutput, RouteResult},
    settings, tabs, view_loading,
    view_snapshot::VersionedView,
};
use crate::{live_view_restoration, presentation::ViewerApp, render::SwapFeedback};

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
    let settings = settings::load(app)?;
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
    let owner = live_view_restoration::restore(app)?;
    if owner {
        Ok(None)
    } else {
        view_loading::ensure_active_view(app)
    }
}
