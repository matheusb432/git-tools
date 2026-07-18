//! `GET /health` — the identity handshake the client uses to decide whether the
//! running daemon matches the freshly-installed exe.

use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::DaemonState;

/// The startup-captured identity of the running daemon.
#[derive(Serialize)]
pub struct Health {
    pub pid: u32,
    pub version: String,
    pub exe_len: u64,
    pub exe_modified_ms: u64,
}

/// Returns `200 OK` with the daemon's startup identity.
pub async fn handle(State(state): State<DaemonState>) -> Json<Health> {
    Json(Health {
        pid: state.pid,
        version: state.version.to_string(),
        exe_len: state.identity.exe_len,
        exe_modified_ms: state.identity.exe_modified_ms,
    })
}
