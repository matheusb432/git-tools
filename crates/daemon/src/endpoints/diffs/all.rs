//! `POST /diffs/all` — mirrors `endpoints/diffs/render.rs` exactly.

use std::sync::Arc;

use application::diffs::render_diff_all::RenderDiffAll;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffAllRequest, RenderDiffData},
    envelope::Envelope,
};
use cqrsy::Dispatcher;

use crate::state::Shared;

/// Renders a diff-all preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<RenderDiffAllRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>)
where
    H: Dispatcher<RenderDiffAll> + Clone + Send + Sync + 'static,
{
    super::run(
        handler,
        shared,
        Ok(super::to_all_request(dto)),
        super::to_all_envelope,
    )
    .await
}
