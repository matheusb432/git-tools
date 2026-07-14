//! `POST /managed/pull-all`. Same shape as `push_all::handle`.

use application::managed::pull_all;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    envelope::Envelope,
    managed::{PullAllRequest, SyncData},
};

use crate::state::DaemonState;

pub async fn handle(
    State(state): State<DaemonState>,
    Json(dto): Json<PullAllRequest>,
) -> (StatusCode, Json<Envelope<SyncData>>) {
    state.shared.touch();
    let req = super::to_pull_all_request(dto);
    match pull_all::execute(req, &state.remote, &state.manifest).await {
        Ok(resp) => (StatusCode::OK, Json(super::to_pull_all_envelope(resp))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::endpoints::error_envelope(format!("{e:#}"))),
        ),
    }
}
