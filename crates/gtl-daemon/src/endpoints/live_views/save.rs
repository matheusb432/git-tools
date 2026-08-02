//! `POST /live-views/save` — validate a directory as a git repo and persist a
//! live view for it, off the blocking pool because repository discovery and
//! `SQLite` access are synchronous.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_contracts::{
    envelope::Envelope,
    live_views::{SaveLiveViewData, SaveLiveViewRequest},
};

use crate::{endpoints::EndpointError, state::DaemonState};

/// Saves a live view for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on a successful save.
/// - `200` with an `error` envelope when the path is rejected (a models outcome, not a request
///   failure) — the rejection message rides as the error note.
/// - `400` with an error envelope when the JSON is invalid.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<SaveLiveViewRequest>, JsonRejection>,
) -> Result<Json<Envelope<SaveLiveViewData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let request = super::to_request(request);
    let response = tokio::task::spawn_blocking(move || {
        gtl_application::live_views::save::execute(
            request,
            &state.git,
            &state.app_state,
            &state.clock,
        )
    })
    .await
    .map_err(EndpointError::task_join)?
    .map_err(EndpointError::unexpected)?;
    Ok(Json(super::to_envelope(response)))
}
