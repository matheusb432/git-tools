//! `POST /managed/push-all`. Genuinely async (real `.await` inside the handler,
//! per `RemoteSync`'s `tokio::process::Command` adapter) — no `spawn_blocking`.

use std::sync::Arc;

use application::managed::push_all::PushAll;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    envelope::Envelope,
    managed::{PushAllRequest, SyncData},
};
use cqrsy::{Handle, Sender};

use crate::state::Shared;

pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<PushAllRequest>,
) -> (StatusCode, Json<Envelope<SyncData>>)
where
    H: Sender<PushAll> + Handle,
{
    shared.touch();
    let req = super::to_push_all_request(dto);
    match handler.send(req).await {
        Ok(resp) => (StatusCode::OK, Json(super::to_push_all_envelope(resp))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(super::error_envelope(format!("{e:#}"))),
        ),
    }
}
