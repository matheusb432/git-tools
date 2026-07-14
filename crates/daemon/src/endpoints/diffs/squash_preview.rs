//! `POST /diffs/squash-preview` — mirrors `endpoints/diffs/render.rs` exactly.

use application::diffs::render_squash_preview;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderSquashPreviewRequest},
    envelope::Envelope,
};

use crate::state::DaemonState;

/// Renders a squash-preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<RenderSquashPreviewRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>) {
    let shared = state.shared.clone();
    crate::endpoints::run(
        shared,
        Ok(super::to_squash_request(dto)),
        move |request| {
            render_squash_preview::execute(
                request,
                &state.source,
                &state.artifacts,
                &state.renderer,
                &state.clock,
            )
        },
        |response| super::to_squash_envelope(&response),
    )
    .await
}
