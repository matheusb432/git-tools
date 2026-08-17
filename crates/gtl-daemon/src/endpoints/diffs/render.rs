//! `POST /diffs/render` — execute a render off the async
//! worker threads (the diff engine shells out to git synchronously).

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_application::diffs::{
    render_diff,
    render_diff::{RenderDiff, RenderDiffError},
};
use gtl_wire::{diffs::RenderDiffData, envelope::Envelope};

use crate::{endpoints::EndpointError, state::DaemonState};

/// Renders a diff for the request body, returning the wire envelope.
///
/// - `200` with an `ok`/`empty` envelope on success.
/// - `400` with an error envelope when the JSON or target selection is invalid.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<RenderDiff>, JsonRejection>,
) -> Result<Json<Envelope<RenderDiffData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let response = tokio::task::spawn_blocking(move || {
        render_diff::execute(
            request,
            &state.user_settings,
            &state.git,
            &state.artifacts,
            &state.renderer,
            &state.clock,
        )
    })
    .await
    .map_err(EndpointError::task_join)?
    .map_err(|error| match error {
        RenderDiffError::InvalidTarget(error) => EndpointError::bad_request(error),
        RenderDiffError::Settings(error) => EndpointError::unexpected(error),
        RenderDiffError::Unexpected(error) => EndpointError::unexpected(error),
    })?;
    Ok(Json(super::to_envelope(response)))
}
