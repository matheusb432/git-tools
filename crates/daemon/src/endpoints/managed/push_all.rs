//! `POST /managed/push-all`. Genuinely async (real `.await` inside the handler,
//! per `RemoteSync`'s `tokio::process::Command` adapter) — no `spawn_blocking`.

use application::managed::push_all;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    envelope::Envelope,
    managed::{PushAllRequest, SyncData},
};

use crate::state::DaemonState;

pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<PushAllRequest>,
) -> (StatusCode, Json<Envelope<SyncData>>) {
    state.shared.touch();
    let req = super::to_push_all_request(dto);
    match push_all::execute(
        req,
        &state.remote,
        &state.manifest,
        &state.ledger,
        &state.clock,
    )
    .await
    {
        Ok(resp) => (StatusCode::OK, Json(super::to_push_all_envelope(resp))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::endpoints::error_envelope(format!("{e:#}"))),
        ),
    }
}
