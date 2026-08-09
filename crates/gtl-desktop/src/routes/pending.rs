use super::{
    response::{RouteError, RouteOutput, RouteResult},
    settings, tabs,
};
use crate::{presentation::ViewerApp, render::SwapFeedback};

pub(super) fn serve(app: &ViewerApp) -> RouteResult {
    app.process_pending_recipes()
        .map_err(|error| RouteError::Internal(error.to_string()))?;
    let settings = settings::load(app)?;
    let html = tabs::render_tabs_only(app.renderer, &app.session, settings, SwapFeedback::None)?;
    Ok(RouteOutput::Html(html))
}
