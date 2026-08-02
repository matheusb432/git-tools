//! `POST /shutdown` — accept the request, then trigger graceful shutdown.

use axum::{extract::State, http::StatusCode};

use crate::state::DaemonState;

/// Signals graceful shutdown and returns `202 Accepted`. The serve loop drains
/// in-flight requests before the process exits.
pub async fn handle(State(state): State<DaemonState>) -> StatusCode {
    let _ = state.shutdown_tx.send(true);
    StatusCode::ACCEPTED
}
