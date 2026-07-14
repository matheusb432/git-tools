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

/// Returns `200 OK` with the daemon's startup identity. Also counts as activity
/// so a health-poll keeps an otherwise-idle daemon alive.
pub async fn handle(State(state): State<DaemonState>) -> Json<Health> {
    state.shared.touch();
    Json(Health {
        pid: state.shared.pid,
        version: state.shared.version.to_string(),
        exe_len: state.shared.identity.exe_len,
        exe_modified_ms: state.shared.identity.exe_modified_ms,
    })
}
