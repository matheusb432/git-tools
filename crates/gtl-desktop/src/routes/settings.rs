use gtl_application::{
    ports::UserSettingsStore,
    viewer::{RenderOptions, Theme, ViewerSettings},
};
use tauri::http::StatusCode;

use super::{
    SettingChange,
    response::{RouteOutput, RouteResult},
};
use crate::presentation::ViewerApp;

const LAYOUT_KEY: &str = "layout";
const DENSITY_KEY: &str = "density";
const THEME_KEY: &str = "theme";

pub(super) fn serve(app: &ViewerApp, change: SettingChange) -> RouteResult {
    let (key, value) = match change {
        SettingChange::Layout(value) => (LAYOUT_KEY, value.to_string()),
        SettingChange::Density(value) => (DENSITY_KEY, value.to_string()),
        SettingChange::Theme(value) => (THEME_KEY, value.to_string()),
    };
    persist(app, key, value)?;
    Ok(RouteOutput::Empty(StatusCode::NO_CONTENT))
}

pub(super) fn load(app: &ViewerApp) -> ViewerSettings {
    let settings = app.user_settings.load();
    let theme = settings
        .theme()
        .and_then(|value| value.parse().ok())
        .unwrap_or(Theme::Dark);
    ViewerSettings::new(settings.viewer_render_options(), theme)
}

pub(super) fn persist_render_options(
    app: &ViewerApp,
    options: RenderOptions,
) -> Result<(), String> {
    persist(app, LAYOUT_KEY, options.layout().to_string())?;
    persist(app, DENSITY_KEY, options.density().to_string())
}

fn persist(app: &ViewerApp, key: &str, value_new: String) -> Result<(), String> {
    gtl_application::settings::set_key::execute(
        gtl_application::settings::set_key::SetSettingKey {
            key: key.into(),
            value_new,
        },
        &app.user_settings,
    )
    .map(|_| ())
    .map_err(|error| format!("{error:#}"))
}
