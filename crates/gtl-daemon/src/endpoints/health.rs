//! `GET /health` — the identity handshake the client uses to decide whether the
//! running daemon matches the freshly-installed exe.

use axum::{Json, extract::State};
use gtl_wire::daemon::Health;

use crate::state::DaemonState;

/// Returns `200 OK` with the daemon's startup identity.
pub async fn handle(State(state): State<DaemonState>) -> Json<Health> {
    Json(Health {
        pid: state.pid,
        version: state.version.to_string(),
        exe_identity: state.identity,
    })
}
