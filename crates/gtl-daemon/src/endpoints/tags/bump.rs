//! `POST /tags/bump` revalidates and applies one displayed tag mutation.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_application::tags::bump_tag::BumpTagOk;
use gtl_contracts::{
    envelope::Envelope,
    tags::{BumpTagData, BumpTagRequest},
};

use crate::{endpoints::EndpointError, state::DaemonState};

pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<BumpTagRequest>, JsonRejection>,
) -> Result<Json<Envelope<BumpTagData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let request = super::to_bump_request(request)?;
    let response = tokio::task::spawn_blocking(move || {
        gtl_application::tags::bump_tag::execute(request, &state.git)
    })
    .await
    .map_err(EndpointError::task_join)?
    .map_err(EndpointError::unexpected)?;
    match response {
        BumpTagOk::Rejected { detail } => Err(EndpointError::conflict(detail)),
        applied @ BumpTagOk::Applied { .. } => Ok(Json(super::to_bump_envelope(applied))),
    }
}
