//! `GET /health` — the identity handshake the client uses to decide whether the
//! running daemon matches the freshly-installed exe.

use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::Shared;

/// The startup-captured identity of the running daemon.
#[derive(Serialize)]
pub struct Health {
    pub pid: u32,
    pub version: String,
    pub exe_len: u64,
    pub exe_modified_ms: u64,
}

/// Returns `200 OK` with the daemon's startup identity. Also counts as activity
/// so a health-poll keeps an otherwise-idle daemon alive.
pub async fn handle(State(shared): State<Arc<Shared>>) -> Json<Health> {
    shared.touch();
    Json(Health {
        pid: shared.pid,
        version: shared.version.to_string(),
        exe_len: shared.identity.exe_len,
        exe_modified_ms: shared.identity.exe_modified_ms,
    })
}
