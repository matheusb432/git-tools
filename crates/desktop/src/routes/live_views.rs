use application::live_views::remove::RemoveLiveView;
use domain::viewer::ViewerTabId;
use tauri::http::StatusCode;

use super::{
    RouteError, RouteMediator, RouteResult, ViewerApp, ensure_active_view, html_response,
    load_settings, render, status_response,
};

pub(super) fn delete<M: RouteMediator>(app: &ViewerApp<M>, tab: ViewerTabId) -> RouteResult {
    let source = {
        let session = app.session.lock().map_err(|error| error.to_string())?;
        session.live_source(tab)
    };
    let Some(source) = source else {
        return Ok(status_response(StatusCode::NOT_FOUND));
    };

    app.mediator
        .send_now(RemoveLiveView {
            data_root: (*app.data_root).clone(),
            source_kind: source.kind().into(),
            source_value: source.value(),
        })
        .map_err(|error| format!("{error:#}"))?;

    let closed = app
        .session
        .lock()
        .map_err(|error| error.to_string())?
        .close_live_view(tab, &source);
    if !closed {
        return Err(RouteError::Conflict);
    }

    let transient = ensure_active_view(app)?;
    let settings = load_settings(app)?;
    render::tabs_with_view_after_live_delete(app.renderer, &app.session, transient, settings)
        .map(html_response)
        .map_err(Into::into)
}
