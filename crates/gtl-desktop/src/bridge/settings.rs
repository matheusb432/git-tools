use gtl_application::settings::{
    get_user_settings::{self, GetUserSettings},
    set_key::{self, SetSettingKey},
};
use gtl_contracts::viewer::{
    ViewerApiError, ViewerDiffExclusions, ViewerProjectDiffExclusions, ViewerResource,
    ViewerUserSettings,
};
use gtl_models::settings::UserSettings;

use super::{internal, shell, unavailable};
use crate::presentation::ViewerApp;

pub(super) fn with_current<T>(
    app: &ViewerApp,
    operation: impl FnOnce(
        &gtl_infra::user_config::TomlSettingsStore,
        &UserSettings,
    ) -> Result<T, ViewerApiError>,
) -> Result<T, ViewerApiError> {
    let store = app
        .user_settings
        .lock()
        .map_err(|error| internal("failed to lock user settings", error))?;
    let settings = get_user_settings::execute(GetUserSettings, &*store)
        .map(|response| response.settings)
        .map_err(|error| {
            unavailable(
                ViewerResource::Settings,
                "failed to load user settings",
                error,
            )
        })?;
    operation(&store, &settings)
}

pub(super) fn load(app: &ViewerApp) -> Result<UserSettings, ViewerApiError> {
    with_current(app, |_store, settings| Ok(settings.clone()))
}

pub(super) fn set_root_key(
    app: &ViewerApp,
    key: String,
    value_new: String,
) -> Result<(), ViewerApiError> {
    let mut store = app
        .user_settings
        .lock()
        .map_err(|error| internal("failed to lock user settings", error))?;
    set_key::execute(SetSettingKey { key, value_new }, &mut *store)
        .map(|_| ())
        .map_err(|error| internal("failed to persist viewer preference", error))
}

pub(super) fn get(app: &ViewerApp) -> Result<ViewerUserSettings, ViewerApiError> {
    with_current(app, |store, settings| {
        Ok(to_user_settings(
            settings,
            store.path().map(|path| path.display().to_string()),
        ))
    })
}

fn to_user_settings(
    settings: &UserSettings,
    configuration_path: Option<String>,
) -> ViewerUserSettings {
    let configured_theme = settings.theme().map(shell::to_theme);
    let exclusions = settings.diff_exclusions();
    ViewerUserSettings {
        configuration_path,
        configured_theme,
        effective_theme: configured_theme.unwrap_or(gtl_contracts::viewer::ViewerTheme::Dark),
        render_options: shell::to_render_options(settings.viewer_render_options()),
        push_confirmation_required: settings.push_confirmation_required(),
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: exclusions.default_exclusions().extensions().to_vec(),
            projects: exclusions
                .project_exclusions()
                .map(|(project_name, extensions)| ViewerProjectDiffExclusions {
                    project_name: project_name.to_owned(),
                    extensions: extensions.extensions().to_vec(),
                })
                .collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::DiffExclusions,
        settings::UserSettings,
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    use super::to_user_settings;

    #[test]
    fn readonly_settings_preserve_defaults_projects_and_effective_theme() {
        let settings = UserSettings::new(
            Some(Theme::Hearth),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            false,
            DiffExclusions::new(
                [
                    ("defaults".into(), vec!["md"]),
                    ("git-tools".into(), vec!["lock", "js"]),
                ],
                None,
            ),
        );

        let mapped = to_user_settings(&settings, Some("/config/git-tools.toml".into()));

        assert_eq!(
            mapped.configuration_path.as_deref(),
            Some("/config/git-tools.toml")
        );
        assert_eq!(mapped.configured_theme, Some(mapped.effective_theme));
        assert!(!mapped.push_confirmation_required);
        assert_eq!(mapped.diff_exclusions.default_extensions, ["md"]);
        assert_eq!(mapped.diff_exclusions.projects.len(), 1);
        assert_eq!(
            mapped.diff_exclusions.projects[0].extensions,
            ["js", "lock"]
        );
    }
}
