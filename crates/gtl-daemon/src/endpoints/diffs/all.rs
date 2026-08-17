//! `POST /diffs/all` — mirrors `endpoints/diffs/render.rs` exactly.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_application::projects::render_project_diff::{self, RenderProjectDiff};
use gtl_wire::{diffs::RenderDiffData, envelope::Envelope};

use crate::{endpoints::EndpointError, state::DaemonState};

/// Renders a diff-all artifact for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `400` with an error envelope when the JSON is invalid.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<RenderProjectDiff>, JsonRejection>,
) -> Result<Json<Envelope<RenderDiffData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let response = tokio::task::spawn_blocking(move || {
        render_project_diff::execute(
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
    Ok(Json(super::to_all_envelope(&response)))
}
