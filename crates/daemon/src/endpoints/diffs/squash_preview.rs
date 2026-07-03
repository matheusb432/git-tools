//! `POST /diffs/squash-preview` — mirrors `endpoints/diffs/render.rs` exactly.

use std::sync::Arc;

use application::diffs::render_squash_preview::RenderSquashPreview;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderSquashPreviewRequest},
    envelope::Envelope,
};
use cqrsy::Dispatcher;

use crate::state::Shared;

/// Renders a squash-preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<RenderSquashPreviewRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>)
where
    H: Dispatcher<RenderSquashPreview> + Clone + Send + Sync + 'static,
{
    super::run(
        handler,
        shared,
        Ok(super::to_squash_request(dto)),
        super::to_squash_envelope,
    )
    .await
}
