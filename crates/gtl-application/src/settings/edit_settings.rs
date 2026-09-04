use thiserror::Error;

use crate::{
    ports::{UserSettingsEditError, UserSettingsEditor},
    viewer::{ViewerState, ViewerStateError},
};

#[derive(Debug, Error)]
pub enum EditSettingsError {
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
    #[error(transparent)]
    ViewerState(#[from] ViewerStateError),
}

/// Applies one validated settings patch and publishes its combined viewer effect.
#[cqrsy::command]
pub fn execute(
    patch: super::UserSettingsPatch,
    settings_editor: &mut impl UserSettingsEditor,
    viewer_state: &ViewerState,
) -> Result<super::UserSettingChange, EditSettingsError> {
    let viewer_rows_selected = patch.changes_viewer_rows();
    let outcome = settings_editor.edit(patch)?;
    if outcome.changed() {
        viewer_state.mark_shell_changed()?;
    }
    Ok(super::UserSettingChange {
        viewer_rows_changed: outcome.changed() && viewer_rows_selected,
    })
}
