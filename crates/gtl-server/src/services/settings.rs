use gtl_application::{
    ports::UserSettingsEditError,
    settings::{
        get_user_settings::{self, GetUserSettings, GetUserSettingsError},
        set_setting_key::{self, SetSettingKey, SetSettingKeyError},
    },
};
use gtl_models::{settings::SettingKeyValue, viewer::Theme};
use gtl_wire::v1::{self, settings_service_server::SettingsService};
use tonic::{Request, Response, Status};

use super::{run_blocking, unexpected, user_settings_load_error};
use crate::state::AppState;

#[derive(Clone)]
pub(crate) struct SettingsApi {
    state: AppState,
}

impl SettingsApi {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl SettingsService for SettingsApi {
    async fn get_push_confirmation_requirement(
        &self,
        _request: Request<v1::GetPushConfirmationRequirementRequest>,
    ) -> Result<Response<v1::GetPushConfirmationRequirementResponse>, Status> {
        let store = self.state.user_settings.clone();
        let result = run_blocking(move || get_user_settings::execute(GetUserSettings, &store))
            .await?
            .map_err(|error| match error {
                GetUserSettingsError::Settings(error) => user_settings_load_error(error),
            })?;
        Ok(Response::new(v1::GetPushConfirmationRequirementResponse {
            push_confirmation_required: result.settings.push_confirmation_required(),
        }))
    }

    async fn set_viewer_theme(
        &self,
        request: Request<v1::SetViewerThemeRequest>,
    ) -> Result<Response<v1::SetViewerThemeResponse>, Status> {
        let theme = theme(request.into_inner().theme)?;
        let mut store = self.state.user_settings.clone();
        let path = store
            .path()
            .ok_or_else(|| Status::failed_precondition("user configuration path is unavailable"))?
            .to_path_buf();
        let viewer = self.state.viewer.clone();
        run_blocking(move || {
            set_setting_key::execute(
                SetSettingKey {
                    mutation: SettingKeyValue::Theme(theme),
                },
                &mut store,
                &viewer,
            )
        })
        .await?
        .map_err(set_setting_key_error)?;

        Ok(Response::new(v1::SetViewerThemeResponse {
            theme: wire_theme(theme) as i32,
            configuration_path: path.to_string_lossy().into_owned(),
        }))
    }
}

pub(super) fn set_setting_key_error(error: SetSettingKeyError) -> Status {
    match error {
        SetSettingKeyError::InvalidValueShape { .. }
        | SetSettingKeyError::Settings(
            UserSettingsEditError::InvalidValueShape
            | UserSettingsEditError::InvalidConfiguration { .. },
        ) => {
            tracing::warn!(error = ?error, "user settings cannot be edited");
            Status::failed_precondition("user settings are invalid")
        }
        SetSettingKeyError::Settings(
            UserSettingsEditError::LockTimeout { .. }
            | UserSettingsEditError::ConcurrentModification { .. },
        ) => {
            tracing::warn!(error = ?error, "user settings edit was aborted");
            Status::aborted("user settings edit conflicted with another writer")
        }
        error => unexpected(error, "set user setting"),
    }
}

fn theme(raw: i32) -> Result<Theme, Status> {
    match v1::ViewerTheme::try_from(raw)
        .map_err(|_| Status::invalid_argument("theme is not recognized"))?
    {
        v1::ViewerTheme::Unspecified => Err(Status::invalid_argument("theme is required")),
        v1::ViewerTheme::Light => Ok(Theme::Light),
        v1::ViewerTheme::Dark => Ok(Theme::Dark),
        v1::ViewerTheme::Hearth => Ok(Theme::Hearth),
        v1::ViewerTheme::Mirage => Ok(Theme::Mirage),
        v1::ViewerTheme::Glacier => Ok(Theme::Glacier),
        v1::ViewerTheme::Noir => Ok(Theme::Noir),
        v1::ViewerTheme::Graphite => Ok(Theme::Graphite),
    }
}

fn wire_theme(theme: Theme) -> v1::ViewerTheme {
    match theme {
        Theme::Light => v1::ViewerTheme::Light,
        Theme::Dark => v1::ViewerTheme::Dark,
        Theme::Hearth => v1::ViewerTheme::Hearth,
        Theme::Mirage => v1::ViewerTheme::Mirage,
        Theme::Glacier => v1::ViewerTheme::Glacier,
        Theme::Noir => v1::ViewerTheme::Noir,
        Theme::Graphite => v1::ViewerTheme::Graphite,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_invalid_loaded_settings_to_failed_precondition() {
        let status = user_settings_load_error(
            gtl_application::ports::UserSettingsLoadError::InvalidConfiguration {
                path: "/tmp/config.toml".into(),
                reason: "bad theme".into(),
            },
        );

        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn maps_settings_state_and_concurrency_failures_deliberately() {
        let invalid = set_setting_key_error(SetSettingKeyError::Settings(
            UserSettingsEditError::InvalidConfiguration {
                path: "/tmp/config.toml".into(),
                reason: "bad theme".into(),
            },
        ));
        assert_eq!(invalid.code(), tonic::Code::FailedPrecondition);

        let concurrent = set_setting_key_error(SetSettingKeyError::Settings(
            UserSettingsEditError::ConcurrentModification {
                path: "/tmp/config.toml".into(),
            },
        ));
        assert_eq!(concurrent.code(), tonic::Code::Aborted);
    }
}
