use gtl_application::viewer::{
    get_viewer_file_filters::{self, GetViewerFileFilters},
    set_viewer_file_filters,
};
use gtl_models::failure::Failure;
use gtl_wire::{proto, v1};
use tonic::{Request, Response};

use super::super::{
    run_blocking,
    status::{ApiResult, GrpcResultExt as _, invalid_request, status},
};
use crate::state::AppState;

pub(super) async fn get(
    state: &AppState,
    request: Request<v1::GetViewerFileFiltersRequest>,
) -> ApiResult<v1::GetViewerFileFiltersResponse> {
    let tab_id = proto::viewer::file_filters::decode_get(request.into_inner())
        .map_err(|_| invalid_request("identity"))?;
    let state = state.clone();
    let result = run_blocking(move || {
        get_viewer_file_filters::execute(&GetViewerFileFilters { tab_id }, &state.viewer)
    })
    .await?
    .into_grpc()?;
    Ok(Response::new(proto::viewer::file_filters::encode_filters(
        result,
    )))
}

pub(super) async fn set(
    state: &AppState,
    request: Request<v1::SetViewerFileFiltersRequest>,
) -> ApiResult<v1::SetViewerFileFiltersResponse> {
    let request = proto::viewer::file_filters::decode_set(request.into_inner())
        .map_err(|_| invalid_request("filter"))?;
    let permit = state
        .viewer_file_filter_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| status(&Failure::Busy))?;
    let state = state.clone();
    run_blocking(move || {
        let _permit = permit;
        let prepared = set_viewer_file_filters::prepare(request, &state.viewer)?;
        set_viewer_file_filters::execute(prepared, &state.viewer, &state.git, &state.database)
    })
    .await?
    .into_grpc()?;
    Ok(Response::new(v1::SetViewerFileFiltersResponse {}))
}
