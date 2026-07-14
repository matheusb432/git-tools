//! `POST /diffs/merge` — mirrors `endpoints/diffs/render.rs` exactly.

use application::diffs::render_merge_diff;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderMergeDiffRequest},
    envelope::Envelope,
};

use crate::state::DaemonState;

/// Renders a merge-diff for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<RenderMergeDiffRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>) {
    let shared = state.shared.clone();
    crate::endpoints::run(
        shared,
        Ok(super::to_merge_request(dto)),
        move |request| {
            render_merge_diff::execute(
                request,
                &state.source,
                &state.artifacts,
                &state.renderer,
                &state.clock,
            )
        },
        |response| super::to_merge_envelope(&response),
    )
    .await
}
