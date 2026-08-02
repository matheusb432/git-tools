//! `POST /diffs/squash-preview` — mirrors `endpoints/diffs/render.rs` exactly.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_application::diffs::render_squash_preview::RenderSquashPreview;
use gtl_contracts::{diffs::RenderDiffData, envelope::Envelope};

use crate::{endpoints::EndpointError, state::DaemonState};

/// Renders a squash-preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `400` with an error envelope when the JSON is invalid.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<RenderSquashPreview>, JsonRejection>,
) -> Result<Json<Envelope<RenderDiffData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let response = tokio::task::spawn_blocking(move || {
        gtl_application::diffs::render_squash_preview::execute(
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
    .map_err(EndpointError::unexpected)?;
    Ok(Json(super::to_squash_envelope(&response)))
}
