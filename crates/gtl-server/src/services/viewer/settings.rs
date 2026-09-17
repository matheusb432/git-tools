use gtl_application::{
    ports::UserSettingsEditError,
    settings::{
        edit_settings::{self, EditSettingsError},
        get_user_settings::{self, GetUserSettings},
        set_setting_key,
    },
    viewer,
};
use gtl_models::settings::UserSettings;
use gtl_wire::{proto, v1};
use tonic::{Request, Response, Status};

use super::{
    super::{
        invalid_user_settings_configuration, run_blocking, settings::set_setting_key_error,
        unexpected, user_settings_load_error,
    },
    errors::unexpected_viewer,
    shell::project_shell,
};
use crate::state::AppState;

pub(super) fn get_viewer_settings(
    state: &AppState,
    _request: Request<v1::GetViewerSettingsRequest>,
) -> Result<Response<v1::GetViewerSettingsResponse>, Status> {
    let ((settings, projects_preferences), revision) = state
        .user_settings
        .load_viewer_settings_with_revision()
        .map_err(user_settings_load_error)?;
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
) -> Result<Response<v1::GetSettingsRecoveryResponse>, Status> {
    use gtl_application::ports::UserSettingsRecovery as _;
    let store = state.user_settings.clone();
    let recovery = run_blocking(move || store.inspect())
        .await?
        .map_err(user_settings_load_error)?;
    Ok(Response::new(v1::GetSettingsRecoveryResponse {
        configuration_path: recovery.configuration_path.display().to_string(),
        diagnostic: recovery.diagnostic,
        revision: recovery.revision,
    }))
}

pub(super) async fn reset_settings(
    state: &AppState,
    request: Request<v1::ResetSettingsRequest>,
) -> Result<Response<v1::ResetSettingsResponse>, Status> {
    use gtl_application::settings::reset_settings::{self, ResetSettingsError};
    let revision = request.into_inner().revision;
    if revision.len() != 64 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Status::invalid_argument("settings revision is invalid"));
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
    .map_err(|error| match error {
        ResetSettingsError::Settings(error) => {
            edit_settings_error(EditSettingsError::Settings(error))
        }
        error => unexpected(error, "reset settings"),
    })?;
    state
        .viewer_row_streams
        .cancel_current_stream()
        .map_err(|error| unexpected_viewer(error, "cancel viewer row stream"))?;
    Ok(Response::new(v1::ResetSettingsResponse {
        backup_path: backup.display().to_string(),
    }))
}

pub(super) async fn edit_settings(
    state: &AppState,
    request: Request<v1::EditSettingsRequest>,
) -> Result<Response<v1::EditSettingsResponse>, Status> {
    let request = proto::viewer::decode_edit_settings_request(request.into_inner())
        .map_err(|_| Status::invalid_argument("settings patch is invalid"))?;
    let request = viewer::settings::settings_patch(request)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    let mut store = state.user_settings.clone();
    let viewer = state.viewer.clone();
    let change = run_blocking(move || edit_settings::execute(request, &mut store, &viewer))
        .await?
        .map_err(edit_settings_error)?;
    if change.viewer_rows_changed {
        state
            .viewer_row_streams
            .cancel_current_stream()
            .map_err(|error| unexpected_viewer(error, "cancel viewer row stream"))?;
    }
    Ok(Response::new(v1::EditSettingsResponse {}))
}

pub(super) async fn set_viewer_preference(
    state: &AppState,
    request: Request<v1::SetViewerPreferenceRequest>,
) -> Result<Response<v1::SetViewerPreferenceResponse>, Status> {
    let preference = proto::viewer::decode_set_viewer_preference_request(request.into_inner())
        .map_err(|_| Status::invalid_argument("viewer preference is invalid"))?;
    let mutation = viewer::settings::preference_setting(preference);
    let mut store = state.user_settings.clone();
    let viewer = state.viewer.clone();
    let change = run_blocking(move || set_setting_key::execute(mutation, &mut store, &viewer))
        .await?
        .map_err(set_setting_key_error)?;
    if change.viewer_rows_changed {
        state
            .viewer_row_streams
            .cancel_current_stream()
            .map_err(|error| unexpected_viewer(error, "cancel viewer row stream"))?;
    }
    Ok(Response::new(v1::SetViewerPreferenceResponse {
        shell: Some(project_shell(state, None)?),
    }))
}

pub(super) fn edit_settings_error(error: EditSettingsError) -> Status {
    match error {
        EditSettingsError::Settings(error) => match error {
            UserSettingsEditError::InvalidConfiguration(error) => {
                invalid_user_settings_configuration(&error, "edit settings")
            }
            UserSettingsEditError::Conflict(_) => {
                Status::aborted("user settings edit conflicted with another writer")
            }
            UserSettingsEditError::Adapter(error) => unexpected(error, "edit settings"),
        },
        EditSettingsError::ViewerState(error) => unexpected(error, "edit settings"),
    }
}

pub(super) fn load_user_settings(state: &AppState) -> Result<UserSettings, Status> {
    get_user_settings::execute(GetUserSettings, &state.user_settings).map_err(|error| match error {
        get_user_settings::GetUserSettingsError::Settings(error) => user_settings_load_error(error),
    })
}
