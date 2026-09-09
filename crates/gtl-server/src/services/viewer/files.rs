use gtl_application::viewer::{open_viewer_diff_file, read_viewer_diff_text};
use gtl_wire::{proto, v1};
use tonic::{Request, Response, Status};

use super::{super::run_blocking, errors};
use crate::state::AppState;

pub(super) async fn read_viewer_diff_text(
    state: &AppState,
    request: Request<v1::ReadViewerDiffTextRequest>,
) -> Result<Response<v1::ReadViewerDiffTextResponse>, Status> {
    let request = proto::viewer::text::decode_request(request.into_inner())
        .map_err(|_| Status::invalid_argument("viewer source range is invalid"))?;
    let state = state.clone();
    let lines = run_blocking(move || {
        read_viewer_diff_text::execute(&request, &state.viewer, &state.user_settings)
    })
    .await?
    .map_err(errors::read_diff_text)?;
    Ok(Response::new(proto::viewer::text::encode_response(lines)))
}

pub(super) async fn open_viewer_diff_file(
    state: &AppState,
    request: Request<v1::OpenViewerDiffFileRequest>,
) -> Result<Response<v1::OpenViewerDiffFileResponse>, Status> {
    let request = proto::viewer::decode_open_viewer_diff_file_request(request.into_inner())
        .map_err(|_| Status::invalid_argument("viewer diff file request is invalid"))?;
    let state = state.clone();
    run_blocking(move || {
        open_viewer_diff_file::execute(
            &request,
            &state.viewer,
            &state.user_settings,
            &state.file_system,
            &state.text_editor,
        )
    })
    .await?
    .map_err(errors::open_diff_file)?;
    Ok(Response::new(v1::OpenViewerDiffFileResponse {}))
}
