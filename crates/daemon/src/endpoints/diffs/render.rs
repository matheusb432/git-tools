//! `POST /diffs/render` — dispatch a render through the mediator, off the async
//! worker threads (the diff engine shells out to git synchronously).

use std::sync::Arc;

use application::diffs::render_diff::RenderDiff;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderDiffRequest},
    envelope::Envelope,
};
use cqrsy::{Handle, Sender};

use crate::state::Shared;

/// Renders a diff for the request body, returning the wire envelope.
///
/// - `200` with an `ok`/`empty` envelope on success.
/// - `400` with an error envelope when the DTO cannot be mapped (bad target).
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<RenderDiffRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>)
where
    H: Sender<RenderDiff> + Handle,
{
    super::run(handler, shared, super::to_request(dto), super::to_envelope).await
}
