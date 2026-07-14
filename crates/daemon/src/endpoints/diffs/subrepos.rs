//! `POST /diffs/subrepos` — mirrors `endpoints/diffs/render.rs` exactly.

use application::diffs::render_diff_subrepos;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderDiffSubreposRequest},
    envelope::Envelope,
};

use crate::state::DaemonState;

/// Renders a diff-subrepos preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok`/`empty` envelope on success.
/// - `400` with an error envelope when the DTO cannot be mapped (bad target).
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<RenderDiffSubreposRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>) {
    let shared = state.shared.clone();
    crate::endpoints::run(
        shared,
        super::to_subrepos_request(dto),
        move |request| {
            render_diff_subrepos::execute(
                request,
                &state.source,
                &state.artifacts,
                &state.renderer,
                &state.clock,
            )
        },
        super::to_subrepos_envelope,
    )
    .await
}
