use gtl_wire::viewer::ViewerUserSettings;

use crate::shared::bridge::{ClientApiError, TauriBridge};

const GET_SETTINGS_COMMAND: &str = "viewer_get_settings";

pub(crate) struct UserSettingsApi;

impl UserSettingsApi {
    pub(crate) async fn get_settings() -> Result<ViewerUserSettings, ClientApiError> {
        TauriBridge::invoke(GET_SETTINGS_COMMAND).await
    }
}
