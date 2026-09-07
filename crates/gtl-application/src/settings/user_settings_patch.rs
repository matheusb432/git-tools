use std::collections::BTreeSet;

use gtl_models::{
    diffs::ExcludedExtensions,
    paths::ProjectName,
    settings::{SettingKey, SettingKeyValue},
    viewer::{DiffDensity, DiffLayout, Theme},
};
use thiserror::Error;

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

/// Replaces the settings associated with one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSettingsUpdate {
    /// Identifies the project whose settings are replaced.
    pub name: ProjectName,
    /// Excludes the project from operations over every project when true.
    pub excluded_from_push_all: bool,
    /// Replaces the project's diff-extension exclusions.
    pub diff_exclusions: ExcludedExtensions,
}

/// Reports a duplicate project name in an ordered project-settings replacement.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("project settings contain duplicate project name `{name}`")]
pub struct DuplicateProjectSettingsNameError {
    name: ProjectName,
}

impl DuplicateProjectSettingsNameError {
    /// Returns the repeated project name.
    #[must_use]
    pub const fn name(&self) -> &ProjectName {
        &self.name
    }
}

/// A validated, insertion-ordered replacement for all project settings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectSettingsUpdates(Vec<ProjectSettingsUpdate>);

impl ProjectSettingsUpdates {
    /// Validates that each project name occurs at most once.
    ///
    /// # Errors
    ///
    /// Returns [`DuplicateProjectSettingsNameError`] for the first repeated name.
    pub fn try_new(
        updates: impl IntoIterator<Item = ProjectSettingsUpdate>,
    ) -> Result<Self, DuplicateProjectSettingsNameError> {
        let mut names = BTreeSet::new();
        let mut checked = Vec::new();
        for update in updates {
            if !names.insert(update.name.clone()) {
                return Err(DuplicateProjectSettingsNameError { name: update.name });
            }
            checked.push(update);
        }
        Ok(Self(checked))
    }
}

impl IntoIterator for ProjectSettingsUpdates {
    type Item = ProjectSettingsUpdate;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A complete typed mutation accepted by the user-settings editor port.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserSettingsPatch {
    pub projects_view: UserSettingsFieldUpdate<gtl_models::settings::ProjectsViewMode>,
    /// Changes the configured viewer theme.
    pub theme: UserSettingsFieldUpdate<Theme>,
    /// Changes the configured diff layout.
    pub layout: UserSettingsFieldUpdate<DiffLayout>,
    /// Changes the configured diff density.
    pub density: UserSettingsFieldUpdate<DiffDensity>,
    /// Changes whether pushes require confirmation.
    pub push_confirmation_required: UserSettingsFieldUpdate<bool>,
    /// Changes the default diff-extension exclusions.
    pub default_diff_exclusions: UserSettingsFieldUpdate<ExcludedExtensions>,
    /// Replaces or removes the complete ordered project-settings collection.
    pub projects: UserSettingsFieldUpdate<ProjectSettingsUpdates>,
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
            || !matches!(
                self.default_diff_exclusions,
                UserSettingsFieldUpdate::Unchanged
            )
            || !matches!(self.projects, UserSettingsFieldUpdate::Unchanged)
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

#[cfg(test)]
mod tests {
    use gtl_models::{diffs::ExcludedExtensions, paths::ProjectName};

    use super::{ProjectSettingsUpdate, ProjectSettingsUpdates};

    fn project(name: &str) -> ProjectSettingsUpdate {
        ProjectSettingsUpdate {
            name: ProjectName::try_from(name).unwrap(),
            excluded_from_push_all: false,
            diff_exclusions: ExcludedExtensions::default(),
        }
    }

    #[test]
    fn project_updates_preserve_order_and_reject_duplicate_names() {
        let updates = ProjectSettingsUpdates::try_new([project("zeta"), project("alpha")]).unwrap();
        let names = updates
            .into_iter()
            .map(|update| update.name.to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, ["zeta", "alpha"]);

        let error =
            ProjectSettingsUpdates::try_new([project("same"), project("same")]).unwrap_err();
        assert_eq!(error.name().as_ref(), "same");
    }
}
