//! `POST /live-views/save` — validate a directory as a git repo and persist a
//! live view for it, off the blocking pool (the probe shells out to git and the
//! store hits `SQLite` synchronously).

use std::sync::Arc;

use application::live_views::save::SaveLiveView;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    envelope::Envelope,
    live_views::{SaveLiveViewData, SaveLiveViewRequest},
};
use cqrsy::{Handle, Sender};

use crate::state::Shared;

/// Saves a live view for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on a successful save.
/// - `200` with an `error` envelope when the path is rejected (a domain outcome, not a request
///   failure) — the rejection message rides as the error note.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<SaveLiveViewRequest>,
) -> (StatusCode, Json<Envelope<SaveLiveViewData>>)
where
    H: Sender<SaveLiveView> + Handle,
{
    crate::endpoints::run(
        handler,
        shared,
        Ok(super::to_request(dto)),
        super::to_envelope,
    )
    .await
}
