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
        files_sidebar_visible: application_field_update(request.files_sidebar_visible, |value| {
            value
        }),
        commits_sidebar_visible: application_field_update(
            request.commits_sidebar_visible,
            |value| value,
        ),
        wrap_lines: application_field_update(request.wrap_lines, |value| value),
        projects_view: application_field_update(request.projects_view, |value| value),
        theme: application_field_update(request.theme, |value| match value {
            ViewerTheme::Light => Theme::Light,
            ViewerTheme::Dark => Theme::Dark,
            ViewerTheme::Hearth => Theme::Hearth,
            ViewerTheme::Mirage => Theme::Mirage,
            ViewerTheme::Glacier => Theme::Glacier,
            ViewerTheme::Noir => Theme::Noir,
            ViewerTheme::Graphite => Theme::Graphite,
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
            ViewerTheme::Light => Theme::Light,
            ViewerTheme::Hearth => Theme::Hearth,
            ViewerTheme::Mirage => Theme::Mirage,
            ViewerTheme::Glacier => Theme::Glacier,
            ViewerTheme::Noir => Theme::Noir,
            ViewerTheme::Graphite => Theme::Graphite,
        }),
    }
}

pub fn project_settings(
    settings: &UserSettings,
    projects_view: gtl_models::settings::ProjectsViewMode,
    configuration_path: Option<String>,
) -> ViewerUserSettings {
    let configured_theme = settings.theme().map(super::project_theme);
    let exclusions = settings.diff_exclusions();
    ViewerUserSettings {
        sidebars: settings.sidebar_visibility(),
        projects_view,
        configuration_path,
        configured_theme,
        effective_theme: configured_theme.unwrap_or(ViewerTheme::Dark),
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
