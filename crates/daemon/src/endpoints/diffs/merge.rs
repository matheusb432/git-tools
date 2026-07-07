//! `POST /diffs/merge` — mirrors `endpoints/diffs/render.rs` exactly.

use std::sync::Arc;

use application::diffs::render_merge_diff::RenderMergeDiff;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderMergeDiffRequest},
    envelope::Envelope,
};
use cqrsy::{Handle, Sender};

use crate::state::Shared;

/// Renders a merge-diff for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<RenderMergeDiffRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>)
where
    H: Sender<RenderMergeDiff> + Handle,
{
    super::run(handler, shared, Ok(super::to_merge_request(dto)), |resp| {
        super::to_merge_envelope(&resp)
    })
    .await
}
