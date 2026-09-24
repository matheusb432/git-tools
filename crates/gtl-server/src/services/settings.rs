use gtl_application::settings::{
    get_user_settings::{self, GetUserSettings},
    set_setting_key,
};
use gtl_models::{failure::SettingsFailure, settings::SettingKeyValue, viewer::Theme};
use gtl_wire::v1::{self, settings_service_server::SettingsService};
use tonic::{Request, Response, Status};

use super::{
    run_blocking,
    status::{ApiResult, GrpcResultExt as _, invalid_request, status},
};
use crate::state::AppState;

pub(crate) struct SettingsGrpcService {
    state: AppState,
}

impl SettingsGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl SettingsService for SettingsGrpcService {
    async fn get_push_confirmation_requirement(
        &self,
        _request: Request<v1::GetPushConfirmationRequirementRequest>,
    ) -> ApiResult<v1::GetPushConfirmationRequirementResponse> {
        let store = self.state.user_settings.clone();
        let result = run_blocking(move || get_user_settings::execute(GetUserSettings, &store))
            .await?
            .into_grpc()?;
        Ok(Response::new(v1::GetPushConfirmationRequirementResponse {
            push_confirmation_required: result.push_confirmation_required(),
        }))
    }

    async fn set_viewer_theme(
        &self,
        request: Request<v1::SetViewerThemeRequest>,
    ) -> ApiResult<v1::SetViewerThemeResponse> {
        let theme = theme(request.into_inner().theme)?;
        let mut store = self.state.user_settings.clone();
        let path = store
            .path()
            .ok_or_else(|| status(&SettingsFailure::PathUnavailable))?
            .to_path_buf();
        let viewer = self.state.viewer.clone();
        run_blocking(move || {
            set_setting_key::execute(SettingKeyValue::Theme(theme), &mut store, &viewer)
        })
        .await?
        .into_grpc()?;

        Ok(Response::new(v1::SetViewerThemeResponse {
            theme: wire_theme(theme) as i32,
            configuration_path: path.to_string_lossy().into_owned(),
        }))
    }
}

fn theme(raw: i32) -> Result<Theme, Status> {
    match v1::ViewerTheme::try_from(raw).map_err(|_| invalid_request("theme"))? {
        v1::ViewerTheme::Unspecified => Err(invalid_request("theme")),
        v1::ViewerTheme::Dark => Ok(Theme::Dark),
        v1::ViewerTheme::Mirage => Ok(Theme::Mirage),
        v1::ViewerTheme::Glacier => Ok(Theme::Glacier),
        v1::ViewerTheme::Graphite => Ok(Theme::Graphite),
        v1::ViewerTheme::Carbon => Ok(Theme::Carbon),
    }
}

fn wire_theme(theme: Theme) -> v1::ViewerTheme {
    match theme {
        Theme::Dark => v1::ViewerTheme::Dark,
        Theme::Mirage => v1::ViewerTheme::Mirage,
        Theme::Glacier => v1::ViewerTheme::Glacier,
        Theme::Graphite => v1::ViewerTheme::Graphite,
        Theme::Carbon => v1::ViewerTheme::Carbon,
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::{
        ports::{UserSettingsConfigurationError, UserSettingsEditConflict},
        settings::set_setting_key::SetSettingKeyError,
    };
    use gtl_models::failure::{Failure, SettingsFailure};

    use super::super::status::{decoded_failure, status};

    #[test]
    fn maps_settings_state_and_concurrency_failures_to_typed_reasons() {
        let invalid = status(&SetSettingKeyError::Settings(
            UserSettingsConfigurationError::new("/tmp/config.toml".into(), anyhow::anyhow!("bad"))
                .into(),
        ));
        assert_eq!(invalid.code(), tonic::Code::FailedPrecondition);

        let concurrent = status(&SetSettingKeyError::Settings(
            UserSettingsEditConflict::ConcurrentModification {
                path: "/tmp/config.toml".into(),
            }
            .into(),
        ));
        assert_eq!(concurrent.code(), tonic::Code::Aborted);
        assert_eq!(
            decoded_failure(&concurrent),
            Some(Failure::Settings(SettingsFailure::Stale))
        );

        let locked = status(&SetSettingKeyError::Settings(
            UserSettingsEditConflict::LockTimeout {
                path: "/tmp/config.toml".into(),
                wait_seconds: 5,
            }
            .into(),
        ));
        assert_eq!(
            decoded_failure(&locked),
            Some(Failure::Settings(SettingsFailure::Locked {
                wait_seconds: 5
            }))
        );
    }
}
