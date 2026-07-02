//! `POST /shutdown` — accept the request, then trigger graceful shutdown.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode};

use crate::state::Shared;

/// Signals graceful shutdown and returns `202 Accepted`. The serve loop drains
/// in-flight requests before the process exits.
pub async fn handle(State(shared): State<Arc<Shared>>) -> StatusCode {
    let _ = shared.shutdown_tx.send(true);
    StatusCode::ACCEPTED
}
