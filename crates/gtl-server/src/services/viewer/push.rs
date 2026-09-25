use gtl_application::viewer::push::{
    PushError, PushPlan, create_viewer_push, execute_viewer_push, get_viewer_push,
    get_viewer_push_availability, refresh_push_views,
};
use gtl_models::failure::Failure;
use gtl_wire::{proto, v1, viewer::push::ViewerPushRequest};
use tonic::{Request, Response, Status};

use super::super::{
    run_blocking,
    status::{ApiResult, GrpcResultExt as _, invalid_request, status},
};
use crate::state::AppState;

pub(super) async fn create(
    state: &AppState,
    request: Request<v1::CreateViewerPushRequest>,
) -> ApiResult<v1::CreateViewerPushResponse> {
    let request = proto::viewer::push::decode_create(request.into_inner())
        .map_err(|_| invalid_request("source"))?;
    let permit = state
        .viewer_push_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| status(&Failure::Busy))?;
    let state = state.clone();
    let id = run_blocking(move || {
        let _permit = permit;
        create_viewer_push::execute(
            &request,
            &state.viewer_push_operations,
            &state.viewer,
            &state.user_settings,
            &state.git,
            &state.projects,
        )
    })
    .await?
    .into_grpc()?;
    Ok(Response::new(v1::CreateViewerPushResponse {
        id: id.to_string(),
    }))
}

pub(super) fn get(
    state: &AppState,
    request: Request<v1::GetViewerPushRequest>,
) -> ApiResult<v1::GetViewerPushResponse> {
    let id = proto::viewer::push::decode_id(&request.into_inner().id)
        .map_err(|_| invalid_request("id"))?;
    let result = get_viewer_push::execute(ViewerPushRequest { id }, &state.viewer_push_operations)
        .into_grpc()?;
    Ok(Response::new(proto::viewer::push::encode_status(result)))
}

pub(super) fn start(
    state: &AppState,
    request: Request<v1::StartViewerPushRequest>,
) -> ApiResult<v1::StartViewerPushResponse> {
    let id = proto::viewer::push::decode_id(&request.into_inner().id)
        .map_err(|_| invalid_request("id"))?;
    state.viewer_push_operations.queue(id).into_grpc()?;
    dispatch_ready(state)?;
    Ok(Response::new(v1::StartViewerPushResponse {}))
}

fn dispatch_ready(state: &AppState) -> Result<(), Status> {
    loop {
        let Ok(permit) = state.viewer_push_workers.clone().try_acquire_owned() else {
            return Ok(());
        };
        let Some((id, plan)) = state.viewer_push_operations.begin_next().into_grpc()? else {
            return Ok(());
        };
        // The admitted operation survives an IPC disconnect or a route change.
        tokio::spawn(run_push(state.clone(), id, plan, permit));
    }
}

async fn run_push(
    state: AppState,
    id: gtl_models::viewer::ViewerPushId,
    plan: PushPlan,
    permit: tokio::sync::OwnedSemaphorePermit,
) {
    let worker_state = state.clone();
    let path = plan.path().clone();
    let result = run_blocking(move || execute_viewer_push::execute(&plan, &worker_state.git))
        .await
        .unwrap_or(Err(PushError::Interrupted));
    let _ = state.viewer_push_operations.finish(id, result);
    if let Ok(work) = refresh_push_views::execute(&path, &state.viewer) {
        for work in work {
            crate::viewer_runtime::spawn_recipe(state.clone(), work);
        }
    }
    let _ = state.viewer.mark_shell_changed();
    drop(permit);
    let _ = dispatch_ready(&state);
}

pub(super) async fn availability(
    state: &AppState,
    request: Request<v1::GetViewerPushAvailabilityRequest>,
) -> ApiResult<v1::GetViewerPushAvailabilityResponse> {
    let identity = proto::viewer::push::decode_availability_request(request.into_inner())
        .map_err(|_| invalid_request("identity"))?;
    let permit = state
        .viewer_push_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| status(&Failure::Busy))?;
    let state = state.clone();
    let result = run_blocking(move || {
        let _permit = permit;
        get_viewer_push_availability::execute(
            identity,
            &state.viewer,
            &state.user_settings,
            &state.git,
        )
    })
    .await?;
    Ok(Response::new(proto::viewer::push::encode_availability(
        result,
    )))
}
