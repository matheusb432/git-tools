//! `POST /managed/pull-all`. Same shape as `push_all::handle`.

use std::sync::Arc;

use application::managed::pull_all::PullAll;
use axum::{Json, extract::State, http::StatusCode};
use contracts::{
    envelope::Envelope,
    managed::{PullAllRequest, SyncData},
};
use cqrsy::{Handle, Sender};

use crate::state::Shared;

pub async fn handle<H>(
    State(handler): State<H>,
    State(shared): State<Arc<Shared>>,
    Json(dto): Json<PullAllRequest>,
) -> (StatusCode, Json<Envelope<SyncData>>)
where
    H: Sender<PullAll> + Handle,
{
    shared.touch();
    let req = super::to_pull_all_request(dto);
    match handler.send(req).await {
        Ok(resp) => (StatusCode::OK, Json(super::to_pull_all_envelope(resp))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(crate::endpoints::error_envelope(format!("{e:#}"))),
        ),
    }
}
