use gtl_models::{
    settings::{SettingKeyValue, UserSettings},
    viewer::{DiffDensity, DiffLayout, Theme},
};
use gtl_wire::viewer::{
    EditSettingsRequest, FieldUpdate, SetViewerPreference, ViewerDiffDensity, ViewerDiffExclusions,
    ViewerDiffLayout, ViewerProjectDiffExclusions, ViewerTheme, ViewerUserSettings,
};

use crate::settings::{
    DuplicateProjectSettingsNameError, ProjectSettingsUpdate, ProjectSettingsUpdates,
    UserSettingsFieldUpdate, UserSettingsPatch,
};

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

pub fn settings_patch(
    request: EditSettingsRequest,
) -> Result<UserSettingsPatch, DuplicateProjectSettingsNameError> {
    let projects = match request.projects {
        FieldUpdate::Update(projects) => {
            let mut mapped = Vec::with_capacity(projects.len());
            for project in projects {
                mapped.push(ProjectSettingsUpdate {
                    name: project.project_name,
                    excluded_from_push_all: project.excluded_from_push_all,
                    diff_exclusions: project.diff_exclusions,
                });
            }
            UserSettingsFieldUpdate::Update(ProjectSettingsUpdates::try_new(mapped)?)
        }
        FieldUpdate::Clear => UserSettingsFieldUpdate::Clear,
        FieldUpdate::Unchanged => UserSettingsFieldUpdate::Unchanged,
    };
    Ok(UserSettingsPatch {
        ui_scale_percent: application_field_update(request.ui_scale_percent, |value| value),
        reduce_motion: application_field_update(request.reduce_motion, |value| value),
        language: application_field_update(request.language, |value| value),
        date_format: application_field_update(request.date_format, |value| value),
        expected_revision: request.expected_revision,
        diff_exclusions: None,
        focus_window_on_diff: application_field_update(request.focus_window_on_diff, |value| value),
        files_sidebar_visible: application_field_update(request.files_sidebar_visible, |value| {
            value
        }),
        commits_sidebar_visible: application_field_update(
            request.commits_sidebar_visible,
            |value| value,
        ),
        wrap_lines: application_field_update(request.wrap_lines, |value| value),
        projects_view: application_field_update(request.projects_view, |value| value),
        projects_sort: application_field_update(request.projects_sort, |value| value),
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
        push_confirmation_required: application_field_update(
            request.push_confirmation_required,
            |value| value,
        ),
        default_diff_exclusions: application_field_update(
            request.default_diff_exclusions,
            |value| value,
        ),
        projects,
    })
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
    let exclusions = settings.diff_exclusions();
    ViewerUserSettings {
        accessibility: settings.accessibility(),
        language: settings.language(),
        date_format: settings.date_format(),
        revision,
        focus_window_on_diff: settings.focus_window_on_diff(),
        sidebars: settings.sidebar_visibility(),
        projects_view: projects.view,
        projects_sort: projects.sort,
        projects_page_size: projects.page_size,
        configuration_path,
        configured_theme,
        effective_theme: super::project_theme(settings.theme().unwrap_or_default()),
        render_options: super::project_render_options(settings.viewer_render_options()),
        push_confirmation_required: settings.push_confirmation_required(),
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: exclusions.default_exclusions().clone(),
            projects: {
                let mut names = std::collections::BTreeSet::new();
                names.extend(
                    exclusions
                        .project_exclusions()
                        .map(|(name, _)| name.clone()),
                );
                names.extend(settings.push_all_exclusions().projects().cloned());
                names
                    .into_iter()
                    .map(|project_name| ViewerProjectDiffExclusions {
                        configured: exclusions.for_project(&project_name).is_some(),
                        extensions: exclusions
                            .for_project(&project_name)
                            .cloned()
                            .unwrap_or_default(),
                        excluded_from_push_all: settings
                            .push_all_exclusions()
                            .contains(&project_name),
                        project_name,
                    })
                    .collect()
            },
        },
    }
}

#[derive(Clone)]
pub(super) struct TabSettings<S> {
    source: S,
    excluded: Option<gtl_models::diffs::ExcludedExtensions>,
}

impl<S> TabSettings<S> {
    pub(super) fn new(source: S, excluded: Option<gtl_models::diffs::ExcludedExtensions>) -> Self {
        Self { source, excluded }
    }
}

impl<S: crate::ports::UserSettingsReader> crate::ports::UserSettingsReader for TabSettings<S> {
    fn load(&self) -> Result<UserSettings, crate::ports::UserSettingsLoadError> {
        let settings = self.source.load()?;
        Ok(match &self.excluded {
            Some(excluded) => settings.with_diff_exclusions(
                gtl_models::diffs::DiffExclusions::new([], Some(excluded.extensions().to_vec())),
            ),
            None => settings,
        })
    }
}
