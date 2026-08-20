use gtl_application::{
    ports::UserSettingsStore as _,
    settings::set_setting_key::{self, SetSettingKey},
};
use gtl_models::{settings::SettingKeyValue, viewer::Theme};
use gtl_wire::v1::{self, settings_service_server::SettingsService};
use tonic::{Request, Response, Status};

use super::{task_join, unexpected};
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
    async fn get_settings(
        &self,
        _request: Request<v1::Empty>,
    ) -> Result<Response<v1::GetSettingsResponse>, Status> {
        let store = self.state.user_settings.clone();
        let settings = tokio::task::spawn_blocking(move || store.load())
            .await
            .map_err(|error| task_join(&error))?
            .map_err(|error| unexpected(error, "load user settings"))?;
        Ok(Response::new(v1::GetSettingsResponse {
            push_confirmation_required: settings.push_confirmation_required(),
        }))
    }

    async fn set_theme(
        &self,
        request: Request<v1::SetThemeRequest>,
    ) -> Result<Response<v1::SetThemeResponse>, Status> {
        let theme = theme(request.into_inner().theme)?;
        let mut store = self.state.user_settings.clone();
        let path = store
            .path()
            .ok_or_else(|| Status::failed_precondition("user configuration path is unavailable"))?
            .to_path_buf();
        tokio::task::spawn_blocking(move || {
            set_setting_key::execute(
                SetSettingKey {
                    mutation: SettingKeyValue::Theme(theme),
                },
                &mut store,
            )
        })
        .await
        .map_err(|error| task_join(&error))?
        .map_err(|error| unexpected(error, "set diff artifact theme"))?;

        Ok(Response::new(v1::SetThemeResponse {
            theme: wire_theme(theme) as i32,
            configuration_path: path.to_string_lossy().into_owned(),
        }))
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
