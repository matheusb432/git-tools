//! `POST /diffs/subrepos` — mirrors `endpoints/diffs/render.rs` exactly.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_application::diffs::render_diff_subrepos::{RenderDiffSubrepos, RenderDiffSubreposError};
use gtl_contracts::{diffs::RenderDiffData, envelope::Envelope};

use crate::{endpoints::EndpointError, state::DaemonState};

/// Renders a diff-subrepos preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok`/`empty` envelope on success.
/// - `400` with an error envelope when the JSON or target selection is invalid.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<RenderDiffSubrepos>, JsonRejection>,
) -> Result<Json<Envelope<RenderDiffData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let response = tokio::task::spawn_blocking(move || {
        gtl_application::diffs::render_diff_subrepos::execute(
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
        RenderDiffSubreposError::InvalidTarget(error) => EndpointError::bad_request(error),
        RenderDiffSubreposError::Settings(error) => EndpointError::unexpected(error),
        RenderDiffSubreposError::Unexpected(error) => EndpointError::unexpected(error),
    })?;
    Ok(Json(super::to_subrepos_envelope(response)))
}
