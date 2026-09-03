use gtl_models::{
    diffs::ExcludedExtensions,
    paths::ProjectName,
    viewer::{DiffDensity, DiffLayout, Theme},
};
use thiserror::Error;

use crate::{
    ports::{UserSettingsEditError, UserSettingsStore},
    viewer::{ViewerState, ViewerStateError},
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FieldUpdate<T> {
    Update(T),
    Clear,
    #[default]
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSettingsUpdate {
    pub name: ProjectName,
    pub excluded_from_push_all: bool,
    pub diff_exclusions: ExcludedExtensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditSettingsRequest {
    pub theme: FieldUpdate<Theme>,
    pub layout: FieldUpdate<DiffLayout>,
    pub density: FieldUpdate<DiffDensity>,
    pub push_confirmation_required: FieldUpdate<bool>,
    pub default_diff_exclusions: FieldUpdate<ExcludedExtensions>,
    pub projects: FieldUpdate<Vec<ProjectSettingsUpdate>>,
}

impl EditSettingsRequest {
    const fn changes_viewer_rows(&self) -> bool {
        !matches!(self.layout, FieldUpdate::Unchanged)
            || !matches!(self.density, FieldUpdate::Unchanged)
            || !matches!(self.default_diff_exclusions, FieldUpdate::Unchanged)
            || !matches!(self.projects, FieldUpdate::Unchanged)
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EditSettingsError {
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
    #[error(transparent)]
    ViewerState(#[from] ViewerStateError),
}

/// Applies one validated settings patch and publishes its combined viewer effect.
#[cqrsy::command]
pub fn execute(
    request: EditSettingsRequest,
    settings_store: &mut impl UserSettingsStore,
    viewer_state: &ViewerState,
) -> Result<super::UserSettingChange, EditSettingsError> {
    let viewer_rows_selected = request.changes_viewer_rows();
    let outcome = settings_store.edit_settings(request)?;
    if outcome.changed() {
        viewer_state.mark_shell_changed()?;
    }
    Ok(super::UserSettingChange {
        viewer_rows_changed: outcome.changed() && viewer_rows_selected,
    })
}
