//! `POST /tags/bump/dry-run` computes the exact proposed tag without mutating Git.

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use gtl_application::tags::dry_run_tag_bump;
use gtl_wire::{
    envelope::Envelope,
    tags::{DryRunTagBumpRequest, TagBumpPreview},
};

use crate::{endpoints::EndpointError, state::DaemonState};

pub async fn handle(
    State(state): State<DaemonState>,
    request: Result<Json<DryRunTagBumpRequest>, JsonRejection>,
) -> Result<Json<Envelope<TagBumpPreview>>, EndpointError> {
    let Json(request) = request.map_err(|error| EndpointError::bad_request(error.body_text()))?;
    let request = super::to_dry_run_request(request)?;
    let response =
        tokio::task::spawn_blocking(move || dry_run_tag_bump::execute(request, &state.git))
            .await
            .map_err(EndpointError::task_join)?
            .map_err(EndpointError::unexpected)?;
    Ok(Json(super::to_dry_run_envelope(response)))
}
