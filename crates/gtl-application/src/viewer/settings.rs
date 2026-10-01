use gtl_models::{
    diffs::ExtensionFilter,
    paths::RepositoryRoot,
    settings::{SettingKeyValue, UserSettings},
    viewer::{DiffDensity, DiffLayout, Theme},
};
use gtl_wire::viewer::{
    EditSettingsRequest, FieldUpdate, SetViewerPreference, ViewerDiffDensity, ViewerDiffLayout,
    ViewerTheme, ViewerUserSettings,
};

use crate::settings::{UserSettingsFieldUpdate, UserSettingsPatch};

fn application_field_update<Input, Output>(
    update: FieldUpdate<Input>,
    map: impl FnOnce(Input) -> Output,
) -> UserSettingsFieldUpdate<Output> {
    match update {
        FieldUpdate::Update(value) => UserSettingsFieldUpdate::Update(map(value)),
        FieldUpdate::Clear => UserSettingsFieldUpdate::Clear,
        FieldUpdate::Unchanged => UserSettingsFieldUpdate::Unchanged,
    }
}

#[must_use]
pub fn settings_patch(request: EditSettingsRequest) -> UserSettingsPatch {
    UserSettingsPatch {
        ui_scale_percent: application_field_update(request.ui_scale_percent, |value| value),
        reduce_motion: application_field_update(request.reduce_motion, |value| value),
        language: application_field_update(request.language, |value| value),
        date_format: application_field_update(request.date_format, |value| value),
        expected_revision: request.expected_revision,
        focus_window_on_diff: application_field_update(request.focus_window_on_diff, |value| value),
        files_sidebar_visible: application_field_update(request.files_sidebar_visible, |value| {
            value
        }),
        commits_sidebar_visible: application_field_update(
            request.commits_sidebar_visible,
            |value| value,
        ),
        wrap_lines: application_field_update(request.wrap_lines, |value| value),
        copy_with_line_context: application_field_update(
            request.copy_with_line_context,
            std::convert::identity,
        ),
        projects_sort: application_field_update(request.projects_sort, |value| value),
        diff_files_sort: application_field_update(request.diff_files_sort, |value| value),
        projects_page_size: application_field_update(request.projects_page_size, |value| value),
        theme: application_field_update(request.theme, |value| match value {
            ViewerTheme::Dark => Theme::Dark,
            ViewerTheme::Mirage => Theme::Mirage,
            ViewerTheme::Glacier => Theme::Glacier,
            ViewerTheme::Graphite => Theme::Graphite,
            ViewerTheme::Carbon => Theme::Carbon,
        }),
        layout: application_field_update(request.layout, |value| match value {
            ViewerDiffLayout::Unified => DiffLayout::Unified,
            ViewerDiffLayout::Split => DiffLayout::Split,
        }),
        density: application_field_update(request.density, |value| match value {
            ViewerDiffDensity::Compact => DiffDensity::Compact,
            ViewerDiffDensity::Full => DiffDensity::Full,
        }),
        viewer_push_no_confirmation_projects: application_field_update(
            request.viewer_push_no_confirmation_projects,
            |projects| projects.into_iter().collect(),
        ),
        push_confirmation_required: application_field_update(
            request.push_confirmation_required,
            |value| value,
        ),
        viewer_push_confirmation_required: application_field_update(
            request.viewer_push_confirmation_required,
            |value| value,
        ),
    }
}

#[must_use]
pub fn preference_setting(preference: SetViewerPreference) -> SettingKeyValue {
    match preference {
        SetViewerPreference::Layout(layout) => SettingKeyValue::Layout(match layout {
            ViewerDiffLayout::Unified => DiffLayout::Unified,
            ViewerDiffLayout::Split => DiffLayout::Split,
        }),
        SetViewerPreference::Density(density) => SettingKeyValue::Density(match density {
            ViewerDiffDensity::Compact => DiffDensity::Compact,
            ViewerDiffDensity::Full => DiffDensity::Full,
        }),
        SetViewerPreference::Theme(theme) => SettingKeyValue::Theme(match theme {
            ViewerTheme::Dark => Theme::Dark,
            ViewerTheme::Mirage => Theme::Mirage,
            ViewerTheme::Glacier => Theme::Glacier,
            ViewerTheme::Graphite => Theme::Graphite,
            ViewerTheme::Carbon => Theme::Carbon,
        }),
    }
}

pub fn project_settings(
    settings: &UserSettings,
    projects: gtl_models::settings::ProjectsPreferences,
    revision: gtl_models::settings::UserSettingsRevision,
    configuration_path: Option<String>,
) -> ViewerUserSettings {
    let configured_theme = settings.theme().map(super::project_theme);
    ViewerUserSettings {
        accessibility: settings.accessibility(),
        language: settings.language(),
        date_format: settings.date_format(),
        revision,
        focus_window_on_diff: settings.focus_window_on_diff(),
        copy_with_line_context: settings.copy_with_line_context(),
        diff_files_sort: settings.diff_files_sort(),
        sidebars: settings.sidebar_visibility(),
        projects_sort: projects.sort,
        projects_page_size: projects.page_size,
        configuration_path,
        configured_theme,
        effective_theme: super::project_theme(settings.theme().unwrap_or_default()),
        render_options: super::project_render_options(settings.viewer_render_options()),
        push_confirmation: gtl_models::settings::PushConfirmationPreferences {
            cli_required: settings.push_confirmation_required(),
            viewer_required: settings.viewer_push_confirmation_required(),
        },
        viewer_push_no_confirmation_projects: settings
            .viewer_push_no_confirmation_projects()
            .iter()
            .cloned()
            .collect(),
    }
}

/// Reads a tab's own filter once set, and the repository's saved filter before that.
pub(super) struct TabExtensionFilters<'a, F> {
    saved: &'a F,
    tab_filter: Option<ExtensionFilter>,
}

impl<'a, F> TabExtensionFilters<'a, F> {
    pub(super) const fn new(saved: &'a F, tab_filter: Option<ExtensionFilter>) -> Self {
        Self { saved, tab_filter }
    }
}

impl<F: crate::ports::ExtensionFilterReader> crate::ports::ExtensionFilterReader
    for TabExtensionFilters<'_, F>
{
    fn extension_filter(&self, repository: &RepositoryRoot) -> anyhow::Result<ExtensionFilter> {
        match &self.tab_filter {
            Some(filter) => Ok(filter.clone()),
            None => self.saved.extension_filter(repository),
        }
    }
}
