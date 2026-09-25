use gtl_application::{
    settings::{
        edit_settings,
        get_user_settings::{self, GetUserSettings},
        reset_settings, set_setting_key,
    },
    viewer,
};
use gtl_models::settings::UserSettings;
use gtl_wire::{proto, v1};
use tonic::{Request, Response, Status};

use super::{
    super::{
        run_blocking,
        status::{ApiResult, GrpcResultExt as _, invalid_request},
    },
    shell::project_shell,
};
use crate::state::AppState;

pub(super) fn get_viewer_settings(
    state: &AppState,
    _request: Request<v1::GetViewerSettingsRequest>,
) -> ApiResult<v1::GetViewerSettingsResponse> {
    let ((settings, projects_preferences), revision) = state
        .user_settings
        .load_viewer_settings_with_revision()
        .into_grpc()?;
    Ok(Response::new(
        proto::viewer::encode_get_viewer_settings_response(viewer::settings::project_settings(
            &settings,
            projects_preferences,
            revision,
            state
                .user_settings
                .path()
                .map(|path| path.display().to_string()),
        )),
    ))
}

pub(super) async fn get_settings_recovery(
    state: &AppState,
    _request: Request<v1::GetSettingsRecoveryRequest>,
) -> ApiResult<v1::GetSettingsRecoveryResponse> {
    use gtl_application::ports::UserSettingsRecovery as _;
    let store = state.user_settings.clone();
    let recovery = run_blocking(move || store.inspect()).await?.into_grpc()?;
    Ok(Response::new(v1::GetSettingsRecoveryResponse {
        configuration_path: recovery.configuration_path.display().to_string(),
        diagnostic: recovery.diagnostic,
        revision: recovery.revision,
    }))
}

pub(super) async fn reset_settings(
    state: &AppState,
    request: Request<v1::ResetSettingsRequest>,
) -> ApiResult<v1::ResetSettingsResponse> {
    let revision = request.into_inner().revision;
    if revision.len() != 64 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_request("revision"));
    }
    let state = state.clone();
    let backup = run_blocking(move || {
        reset_settings::execute(
            &revision,
            &mut state.user_settings.clone(),
            &state.clock,
            &state.viewer,
        )
    })
    .await?
    .into_grpc()?;
    state
        .viewer_row_streams
        .cancel_current_stream()
        .into_grpc()?;
    Ok(Response::new(v1::ResetSettingsResponse {
        backup_path: backup.display().to_string(),
    }))
}

pub(super) async fn edit_settings(
    state: &AppState,
    request: Request<v1::EditSettingsRequest>,
) -> ApiResult<v1::EditSettingsResponse> {
    let request = proto::viewer::decode_edit_settings_request(request.into_inner())
        .map_err(|error| invalid_request(error.field().unwrap_or("patch")))?;
    let request = viewer::settings::settings_patch(request);
    let mut store = state.user_settings.clone();
    let viewer = state.viewer.clone();
    let change = run_blocking(move || edit_settings::execute(request, &mut store, &viewer))
        .await?
        .into_grpc()?;
    if change.viewer_rows_changed {
        state
            .viewer_row_streams
            .cancel_current_stream()
            .into_grpc()?;
    }
    Ok(Response::new(v1::EditSettingsResponse {}))
}

pub(super) async fn set_viewer_preference(
    state: &AppState,
    request: Request<v1::SetViewerPreferenceRequest>,
) -> ApiResult<v1::SetViewerPreferenceResponse> {
    let preference = proto::viewer::decode_set_viewer_preference_request(request.into_inner())
        .map_err(|_| invalid_request("preference"))?;
    let mutation = viewer::settings::preference_setting(preference);
    let mut store = state.user_settings.clone();
    let viewer = state.viewer.clone();
    let change = run_blocking(move || set_setting_key::execute(mutation, &mut store, &viewer))
        .await?
        .into_grpc()?;
    if change.viewer_rows_changed {
        state
            .viewer_row_streams
            .cancel_current_stream()
            .into_grpc()?;
    }
    Ok(Response::new(v1::SetViewerPreferenceResponse {
        shell: Some(project_shell(state)?),
    }))
}

pub(super) fn load_user_settings(state: &AppState) -> Result<UserSettings, Status> {
    get_user_settings::execute(GetUserSettings, &state.user_settings).into_grpc()
}
