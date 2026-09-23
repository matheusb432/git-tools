use gtl_application::viewer::{
    file_filters::FileFiltersError,
    get_viewer_file_filters::{self, GetViewerFileFilters},
    set_viewer_file_filters, update_diff_exclusions,
};
use gtl_wire::{proto, v1};
use tonic::{Request, Response, Status};

use super::super::{run_blocking, unexpected, user_settings_load_error};
use crate::state::AppState;

fn error(value: FileFiltersError) -> Status {
    match value {
        FileFiltersError::TooLarge => {
            Status::resource_exhausted("the revealed files exceed the viewer cache limit")
        }
        FileFiltersError::Edit(value) => super::settings::edit_settings_error(value),
        FileFiltersError::Settings(value) => user_settings_load_error(value),
        FileFiltersError::Changed => Status::aborted("the viewer changed"),
        value => unexpected(value, "change diff exclusions"),
    }
}

pub(super) async fn get(
    state: &AppState,
    request: Request<v1::GetViewerFileFiltersRequest>,
) -> Result<Response<v1::GetViewerFileFiltersResponse>, Status> {
    let tab_id = proto::viewer::file_filters::decode_get(request.into_inner())
        .map_err(|_| Status::invalid_argument("invalid viewer identity"))?;
    let state = state.clone();
    let result = run_blocking(move || {
        let connection = state
            .database
            .connection_lock()
            .map_err(FileFiltersError::Database)?;
        get_viewer_file_filters::execute(
            &GetViewerFileFilters { tab_id },
            &state.viewer,
            &state.user_settings,
            &connection,
        )
    })
    .await?
    .map_err(error)?;
    Ok(Response::new(proto::viewer::file_filters::encode_filters(
        result,
    )))
}

pub(super) async fn set(
    state: &AppState,
    request: Request<v1::SetViewerFileFiltersRequest>,
) -> Result<Response<v1::SetViewerFileFiltersResponse>, Status> {
    let request = proto::viewer::file_filters::decode_set(request.into_inner())
        .map_err(|_| Status::invalid_argument("invalid file exclusions"))?;
    let permit = state
        .viewer_file_filter_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| Status::resource_exhausted("file exclusions are already changing"))?;
    let state = state.clone();
    run_blocking(move || {
        let _permit = permit;
        let prepared = {
            let connection = state
                .database
                .connection_lock()
                .map_err(FileFiltersError::Database)?;
            set_viewer_file_filters::prepare(
                request,
                &state.viewer,
                &state.user_settings,
                &connection,
            )?
        };
        set_viewer_file_filters::execute(
            prepared,
            &state.viewer,
            &mut state.user_settings.clone(),
            &state.git,
        )
    })
    .await?
    .map_err(error)?;
    Ok(Response::new(v1::SetViewerFileFiltersResponse {}))
}

pub(super) async fn defaults(
    state: &AppState,
    request: Request<v1::UpdateDiffExclusionsRequest>,
) -> Result<Response<v1::UpdateDiffExclusionsResponse>, Status> {
    let request = proto::viewer::file_filters::decode_defaults(request.into_inner())
        .map_err(|_| Status::invalid_argument("invalid file exclusions"))?;
    let state = state.clone();
    run_blocking(move || {
        update_diff_exclusions::execute(request, &mut state.user_settings.clone(), &state.viewer)
    })
    .await?
    .map_err(error)?;
    Ok(Response::new(v1::UpdateDiffExclusionsResponse {}))
}
