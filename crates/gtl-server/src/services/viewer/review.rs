use gtl_application::diffs::set_diff_file_reviewed;
use gtl_wire::{proto, v1};
use tonic::{Request, Response};

use super::super::{
    run_blocking,
    status::{ApiResult, GrpcResultExt as _, invalid_request},
    unexpected,
};
use crate::state::AppState;

pub(super) async fn set(
    state: &AppState,
    request: Request<v1::SetDiffFileReviewedRequest>,
) -> ApiResult<v1::SetDiffFileReviewedResponse> {
    let request = proto::diff_review::decode_set(request.into_inner())
        .map_err(|_| invalid_request("file_review"))?;
    let permit = state
        .viewer_review_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| super::super::status::status(&gtl_models::failure::Failure::Busy))?;
    let state = state.clone();
    run_blocking(move || {
        let _permit = permit;
        let mut connection = state
            .database
            .connection_lock()
            .map_err(|error| unexpected(error, "save review progress"))?;
        set_diff_file_reviewed::execute(&request, &mut connection, &state.viewer).into_grpc()
    })
    .await??;
    Ok(Response::new(v1::SetDiffFileReviewedResponse {}))
}
