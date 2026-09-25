use gtl_models::{
    settings::{SettingKey, SettingKeyValue, UserSettingsRevision},
    viewer::{DiffDensity, DiffLayout, Theme},
};

/// Selects how one optional user-settings field should change.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UserSettingsFieldUpdate<T> {
    /// Replaces the field with a validated value.
    Update(T),
    /// Removes the field so its default or absence applies.
    Clear,
    /// Leaves the field exactly as persisted.
    #[default]
    Unchanged,
}

/// A complete typed mutation accepted by the user-settings editor port.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserSettingsPatch {
    pub ui_scale_percent: UserSettingsFieldUpdate<gtl_models::settings::ViewerScalePercent>,
    pub reduce_motion: UserSettingsFieldUpdate<bool>,
    pub language: UserSettingsFieldUpdate<gtl_models::settings::ViewerLanguage>,
    pub date_format: UserSettingsFieldUpdate<gtl_models::settings::ViewerDateFormat>,
    /// Rejects the edit when the serialized document changed after it was loaded.
    pub expected_revision: Option<UserSettingsRevision>,
    pub focus_window_on_diff: UserSettingsFieldUpdate<bool>,
    pub files_sidebar_visible: UserSettingsFieldUpdate<bool>,
    pub commits_sidebar_visible: UserSettingsFieldUpdate<bool>,
    pub wrap_lines: UserSettingsFieldUpdate<bool>,
    pub projects_view: UserSettingsFieldUpdate<gtl_models::settings::ProjectsViewMode>,
    pub projects_sort: UserSettingsFieldUpdate<gtl_models::settings::ProjectsSort>,
    pub projects_page_size: UserSettingsFieldUpdate<gtl_models::settings::ProjectsPageSize>,
    /// Changes the configured viewer theme.
    pub theme: UserSettingsFieldUpdate<Theme>,
    /// Changes the configured diff layout.
    pub layout: UserSettingsFieldUpdate<DiffLayout>,
    /// Changes the configured diff density.
    pub density: UserSettingsFieldUpdate<DiffDensity>,
    /// Changes whether pushes require confirmation.
    pub push_confirmation_required: UserSettingsFieldUpdate<bool>,
}

impl UserSettingsPatch {
    /// Clears one supported scalar field so its default applies.
    #[must_use]
    pub fn clear(key: SettingKey) -> Self {
        let mut patch = Self::default();
        match key {
            SettingKey::Theme => patch.theme = UserSettingsFieldUpdate::Clear,
            SettingKey::Layout => patch.layout = UserSettingsFieldUpdate::Clear,
            SettingKey::Density => patch.density = UserSettingsFieldUpdate::Clear,
        }
        patch
    }

    pub(crate) const fn changes_viewer_rows(&self) -> bool {
        !matches!(self.layout, UserSettingsFieldUpdate::Unchanged)
            || !matches!(self.density, UserSettingsFieldUpdate::Unchanged)
    }
}

impl From<SettingKeyValue> for UserSettingsPatch {
    fn from(mutation: SettingKeyValue) -> Self {
        let mut patch = Self::default();
        match mutation {
            SettingKeyValue::Theme(value) => {
                patch.theme = UserSettingsFieldUpdate::Update(value);
            }
            SettingKeyValue::Layout(value) => {
                patch.layout = UserSettingsFieldUpdate::Update(value);
            }
            SettingKeyValue::Density(value) => {
                patch.density = UserSettingsFieldUpdate::Update(value);
            }
        }
        patch
    }
}
