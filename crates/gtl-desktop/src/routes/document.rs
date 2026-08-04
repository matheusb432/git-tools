use std::sync::Mutex;

use gtl_application::{
    history::list_recent_render_page::RecentRenderPageCursor,
    viewer::{ViewerHistoryPage, ViewerSettings},
};

use super::{
    history, live_views,
    response::{RouteOutput, RouteResult},
    settings,
    view_snapshot::{self, GENERATION_ATTEMPTS_MAX, RenderError, VersionedView},
};
use crate::{presentation::ViewerApp, render::MaudViewerRenderer, session::ViewerSession};

pub(super) fn serve(app: &ViewerApp) -> RouteResult {
    let transient = live_views::restore(app)?;
    let settings = settings::load(app)?;
    let history = history::load(app, RecentRenderPageCursor::Newest)?;
    render_document(app.renderer, &app.session, transient, history, settings)
        .map(RouteOutput::Html)
        .map_err(Into::into)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one gathered document snapshot"
)]
fn render_document(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    history: ViewerHistoryPage,
    settings: ViewerSettings,
) -> Result<String, RenderError> {
    let mut transient = transient;
    for _ in 0..GENERATION_ATTEMPTS_MAX {
        let snapshot = match view_snapshot::gather(
            session,
            transient.take(),
            history.clone(),
            settings.clone(),
        ) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let html = renderer.build_deferred_document(&snapshot.document);
        if view_snapshot::is_current(session, snapshot.ticket, snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}
