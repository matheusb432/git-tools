use gtl_application::viewer::push::{
    PushError, PushPlan, create_viewer_push, execute_viewer_push, get_viewer_push,
    get_viewer_push_availability, refresh_push_views,
};
use gtl_wire::{proto, v1, viewer::push::ViewerPushRequest};
use tonic::{Request, Response, Status};

use super::super::run_blocking;
use crate::state::AppState;

fn status(error: &PushError) -> Status {
    match error {
        PushError::NotFound => Status::not_found(error.to_string()),
        PushError::Capacity => Status::resource_exhausted(error.to_string()),
        _ => Status::internal(error.to_string()),
    }
}

pub(super) async fn create(
    state: &AppState,
    request: Request<v1::CreateViewerPushRequest>,
) -> Result<Response<v1::CreateViewerPushResponse>, Status> {
    let request = proto::viewer::push::decode_create(request.into_inner())
        .map_err(|_| Status::invalid_argument("Invalid push source"))?;
    let permit = state
        .viewer_push_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| Status::resource_exhausted("Push preparation is busy"))?;
    let state = state.clone();
    let id = run_blocking(move || {
        let _permit = permit;
        create_viewer_push::execute(
            &request,
            &state.viewer_push_operations,
            &state.viewer,
            &state.user_settings,
            &state.git,
        )
    })
    .await?
    .map_err(|error| status(&error))?;
    Ok(Response::new(v1::CreateViewerPushResponse {
        id: id.to_string(),
    }))
}

pub(super) fn get(
    state: &AppState,
    request: Request<v1::GetViewerPushRequest>,
) -> Result<Response<v1::GetViewerPushResponse>, Status> {
    let id = proto::viewer::push::decode_id(&request.into_inner().id)
        .map_err(|_| Status::invalid_argument("Invalid push ID"))?;
    let result = get_viewer_push::execute(ViewerPushRequest { id }, &state.viewer_push_operations)
        .map_err(|error| status(&error))?;
    Ok(Response::new(proto::viewer::push::encode_status(result)))
}

pub(super) fn start(
    state: &AppState,
    request: Request<v1::StartViewerPushRequest>,
) -> Result<Response<v1::StartViewerPushResponse>, Status> {
    let id = proto::viewer::push::decode_id(&request.into_inner().id)
        .map_err(|_| Status::invalid_argument("Invalid push ID"))?;
    state
        .viewer_push_operations
        .queue(id)
        .map_err(|error| status(&error))?;
    dispatch_ready(state)?;
    Ok(Response::new(v1::StartViewerPushResponse {}))
}

fn dispatch_ready(state: &AppState) -> Result<(), Status> {
    loop {
        let Ok(permit) = state.viewer_push_workers.clone().try_acquire_owned() else {
            return Ok(());
        };
        let next = state
            .viewer_push_operations
            .begin_next()
            .map_err(|error| status(&error))?;
        let Some((id, plan)) = next else {
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
    let result = run_blocking(move || execute_viewer_push::execute(&plan, &worker_state.git)).await;
    let result = result.unwrap_or_else(|error| Err(PushError::Git(error.to_string())));
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
) -> Result<Response<v1::GetViewerPushAvailabilityResponse>, Status> {
    let identity = proto::viewer::push::decode_availability_request(request.into_inner())
        .map_err(|_| Status::invalid_argument("Invalid view identity"))?;
    let permit = state
        .viewer_push_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| Status::resource_exhausted("Push preparation is busy"))?;
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
