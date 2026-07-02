//! `POST /diffs/all` — mirrors `endpoints/diffs/render.rs` exactly.

use std::sync::Arc;

use application::diffs::render_diff_all::RenderDiffAll;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    diffs::{RenderDiffAllRequest, RenderDiffData},
    envelope::Envelope,
};
use cqrs::RequestHandler;

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
    H: RequestHandler<RenderDiffAll> + Clone + Send + Sync + 'static,
{
    shared.touch();
    let req = super::to_all_request(dto);
    // The diff engine shells out to git synchronously — run it on the blocking
    // pool so worker threads stay free.
    let rt = tokio::runtime::Handle::current();
    let joined = tokio::task::spawn_blocking(move || rt.block_on(handler.handle(req))).await;
    let envelope = match joined {
        Ok(Ok(resp)) => return (StatusCode::OK, Json(super::to_all_envelope(resp))),
        Ok(Err(e)) => super::error_envelope(format!("{e:#}")),
        Err(e) => super::error_envelope(format!("daemon task panicked: {e}")),
    };
    (StatusCode::INTERNAL_SERVER_ERROR, Json(envelope))
}
