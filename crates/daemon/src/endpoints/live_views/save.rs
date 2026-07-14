//! `POST /live-views/save` — validate a directory as a git repo and persist a
//! live view for it, off the blocking pool (the probe shells out to git and the
//! store hits `SQLite` synchronously).

use application::live_views::save;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    envelope::Envelope,
    live_views::{SaveLiveViewData, SaveLiveViewRequest},
};

use crate::state::DaemonState;

/// Saves a live view for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on a successful save.
/// - `200` with an `error` envelope when the path is rejected (a domain outcome, not a request
///   failure) — the rejection message rides as the error note.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<SaveLiveViewRequest>,
) -> (StatusCode, Json<Envelope<SaveLiveViewData>>) {
    let shared = state.shared.clone();
    crate::endpoints::run(
        shared,
        Ok(super::to_request(dto)),
        move |request| save::execute(request, &state.probe, &state.app_state, &state.clock),
        super::to_envelope,
    )
    .await
}
