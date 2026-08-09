//! `POST /managed/pull-all`. Same shape as `push_all::handle`.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_contracts::{
    envelope::Envelope,
    managed::{PullAllRequest, SyncData},
};

use crate::{endpoints::EndpointError, state::DaemonState};

pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<PullAllRequest>, JsonRejection>,
) -> Result<Json<Envelope<SyncData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let request = super::to_pull_all_request(&request);
    let response =
        gtl_application::managed::pull_all::execute(request, &state.git, &state.projects)
            .await
            .map_err(EndpointError::unexpected)?;
    Ok(Json(super::to_pull_all_envelope(response)))
}
