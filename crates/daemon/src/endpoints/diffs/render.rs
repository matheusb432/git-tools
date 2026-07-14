//! `POST /diffs/render` — execute a render off the async
//! worker threads (the diff engine shells out to git synchronously).

use application::diffs::render_diff;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderDiffRequest},
    envelope::Envelope,
};

use crate::state::DaemonState;

/// Renders a diff for the request body, returning the wire envelope.
///
/// - `200` with an `ok`/`empty` envelope on success.
/// - `400` with an error envelope when the DTO cannot be mapped (bad target).
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<RenderDiffRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>) {
    let shared = state.shared.clone();
    crate::endpoints::run(
        shared,
        super::to_request(dto, super::config_exclusions()),
        move |request| {
            render_diff::execute(
                request,
                &state.source,
                &state.artifacts,
                &state.renderer,
                &state.clock,
            )
        },
        super::to_envelope,
    )
    .await
}
