//! `POST /diffs/subrepos` — mirrors `endpoints/diffs/render.rs` exactly.

use std::sync::Arc;

use application::diffs::render_diff_subrepos::RenderDiffSubrepos;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffData, RenderDiffSubreposRequest},
    envelope::Envelope,
};
use cqrs::RequestHandler;

use crate::state::Shared;

/// Renders a diff-subrepos preview for the request body, returning the wire envelope.
///
/// - `200` with an `ok`/`empty` envelope on success.
/// - `400` with an error envelope when the DTO cannot be mapped (bad target).
/// - `500` with an error envelope when the handler (or its blocking task) fails.
pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<RenderDiffSubreposRequest>,
) -> (StatusCode, Json<Envelope<RenderDiffData>>)
where
    H: RequestHandler<RenderDiffSubrepos> + Clone + Send + Sync + 'static,
{
    shared.touch();
    let req = match super::to_subrepos_request(dto) {
        Ok(req) => req,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(super::error_envelope(format!("{e:#}"))),
            );
        }
    };
    // The diff engine shells out to git synchronously — run it on the blocking
    // pool so worker threads stay free.
    let rt = tokio::runtime::Handle::current();
    let joined = tokio::task::spawn_blocking(move || rt.block_on(handler.handle(req))).await;
    let envelope = match joined {
        Ok(Ok(resp)) => return (StatusCode::OK, Json(super::to_subrepos_envelope(resp))),
        Ok(Err(e)) => super::error_envelope(format!("{e:#}")),
        Err(e) => super::error_envelope(format!("daemon task panicked: {e}")),
    };
    (StatusCode::INTERNAL_SERVER_ERROR, Json(envelope))
}
