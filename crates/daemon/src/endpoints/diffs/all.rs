//! `POST /diffs/all` — mirrors `endpoints/diffs/render.rs` exactly.

use application::diffs::render_diff_all;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffAllRequest, RenderDiffData},
    envelope::Envelope,
};

use crate::state::DaemonState;

/// Renders a diff-all preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok` envelope on success.
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<RenderDiffAllRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>) {
    let shared = state.shared.clone();
    crate::endpoints::run(
        shared,
        Ok(super::to_all_request(dto, super::config_exclusions())),
        move |request| {
            render_diff_all::execute(
                request,
                &state.source,
                &state.artifacts,
                &state.renderer,
                &state.clock,
            )
        },
        |response| super::to_all_envelope(&response),
    )
    .await
}
