use application::viewer::{RenderOptions, ViewerTabState};
use maud::{PreEscaped, html};
use tauri::http::StatusCode;

use super::{
    response::{RouteError, RouteOutput, RouteResult},
    settings, tabs,
    view_snapshot::VersionedView,
};
use crate::{
    materialization::{ChunkPage, ViewLoadId},
    presentation::ViewerApp,
    session::RENDER_PENDING_REASON,
};

pub(super) fn ready(app: &ViewerApp) -> RouteResult {
    let render_pending = {
        let session = app.session.lock().map_err(|error| error.to_string())?;
        session
            .active()
            .and_then(|id| session.tab(id))
            .is_some_and(|tab| {
                matches!(
                    tab.tab.state(),
                    ViewerTabState::Error { reason } if reason == RENDER_PENDING_REASON
                )
            })
    };
    if render_pending {
        return Ok(RouteOutput::Empty(StatusCode::NO_CONTENT));
    }
    let settings = settings::load(app);
    let load_id = prepare_materialization(app, settings.options())?;
    tabs::render_view_with_tabs(app.renderer, &app.session, None, settings, load_id)
        .map(RouteOutput::Html)
        .map_err(Into::into)
}

pub(super) fn load_next(app: &ViewerApp, load: ViewLoadId) -> RouteResult {
    let page = app.materializations.next(&app.session, load)?;
    Ok(RouteOutput::Html(render_chunk(load, page)))
}

pub(super) fn ensure_active_view(app: &ViewerApp) -> Result<Option<VersionedView>, RouteError> {
    let refresh = {
        let mut session = app.session.lock().map_err(|error| error.to_string())?;
        let Some(id) = session.active() else {
            return Ok(None);
        };
        let state = session.tab(id).map(|tab| tab.tab.state().clone());
        let needs_compute = match state {
            Some(ViewerTabState::Ready) => session.cached_view_snapshot(id).is_none(),
            Some(ViewerTabState::Error { reason }) => reason == RENDER_PENDING_REASON,
            _ => false,
        };
        needs_compute.then_some(id)
    };
    let Some(id) = refresh else {
        return Ok(None);
    };
    app.refresh_recipe(id)?;
    Ok(None)
}

pub(super) fn prepare_materialization(
    app: &ViewerApp,
    options: RenderOptions,
) -> Result<Option<ViewLoadId>, RouteError> {
    app.materializations
        .prepare(&app.session, options)
        .map_err(Into::into)
}

fn render_chunk(load: ViewLoadId, page: ChunkPage) -> String {
    let target = format!("beforeend:#{}", page.chunk.target_id);
    html! {
        div hx-swap-oob=(target) data-chunk-rows=(page.chunk.rows) {
            (PreEscaped(page.chunk.html))
        }
        @if page.has_more {
            div id="viewer-chunk-loader"
                hx-get=(format!("/loads/{load}/next"))
                hx-trigger="load delay:16ms"
                hx-target="this"
                hx-swap="outerHTML" {}
        } @else {
            div id="viewer-chunk-loader" data-complete hidden {}
        }
    }
    .into_string()
}

#[cfg(test)]
mod tests {
    use preview::ViewChunk;

    use super::*;

    #[test]
    fn chunk_response_inserts_rows_and_bounds_the_next_loader() {
        let html = render_chunk(
            ViewLoadId::try_new(7).expect("positive load id"),
            ChunkPage {
                chunk: ViewChunk {
                    target_id: "viewer-diff-2".into(),
                    html: "<div class=\"dl\">row</div>".into(),
                    rows: 1,
                },
                has_more: true,
            },
        );

        assert!(html.contains("hx-swap-oob=\"beforeend:#viewer-diff-2\""));
        assert!(html.contains("data-chunk-rows=\"1\""));
        assert!(html.contains("hx-get=\"/loads/7/next\""));
        assert_eq!(html.matches("id=\"viewer-chunk-loader\"").count(), 1);
    }
}
