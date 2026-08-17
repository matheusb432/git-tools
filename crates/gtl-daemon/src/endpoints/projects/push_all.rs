//! `POST /managed/push-all`. Genuinely async (real `.await` inside the handler,
//! through the consolidated Git client.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_application::projects::push_repositories;
use gtl_wire::{
    envelope::Envelope,
    projects::{PushAllRequest, SyncData},
};

use crate::{endpoints::EndpointError, state::DaemonState};

pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<PushAllRequest>, JsonRejection>,
) -> Result<Json<Envelope<SyncData>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let request = super::to_push_all_request(&request);
    let response = push_repositories::execute(request, &state.git, &state.projects, &state.clock)
        .await
        .map_err(EndpointError::unexpected)?;
    Ok(Json(super::to_push_all_envelope(response)))
}
