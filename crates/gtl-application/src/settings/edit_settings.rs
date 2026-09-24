use gtl_models::failure::ErrorMeta;
use thiserror::Error;

use crate::{
    ports::{UserSettingsEditError, UserSettingsEditor},
    viewer::{ViewerState, ViewerStateError},
};

#[derive(Debug, Error, ErrorMeta)]
pub enum EditSettingsError {
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsEditError),
    #[error(transparent)]
    #[meta(transparent)]
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

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{DiffLayout, ViewerVersion};

    use crate::{
        ports::UserSettingsEditOutcome,
        settings::{
            UserSettingsFieldUpdate, UserSettingsPatch, edit_settings,
            test_support::FixedUserSettingsEditStore,
        },
        viewer::ViewerState,
    };

    fn execute_with_outcome(
        patch: UserSettingsPatch,
        outcome: UserSettingsEditOutcome,
    ) -> (crate::settings::UserSettingChange, ViewerVersion) {
        let mut store = FixedUserSettingsEditStore::new(outcome);
        let viewer = ViewerState::new();
        let response = edit_settings::execute(patch, &mut store, &viewer).unwrap();
        (response, viewer.version().unwrap())
    }

    #[test]
    fn edits_publish_shell_changes_and_invalidate_rows_only_for_row_options() {
        for (patch, outcome, viewer_rows_changed, version) in [
            (
                UserSettingsPatch {
                    push_confirmation_required: UserSettingsFieldUpdate::Update(false),
                    ..UserSettingsPatch::default()
                },
                UserSettingsEditOutcome::Changed,
                false,
                ViewerVersion::new(1),
            ),
            (
                UserSettingsPatch {
                    layout: UserSettingsFieldUpdate::Update(DiffLayout::Split),
                    ..UserSettingsPatch::default()
                },
                UserSettingsEditOutcome::Changed,
                true,
                ViewerVersion::new(1),
            ),
            (
                UserSettingsPatch {
                    layout: UserSettingsFieldUpdate::Update(DiffLayout::Split),
                    ..UserSettingsPatch::default()
                },
                UserSettingsEditOutcome::Unchanged,
                false,
                ViewerVersion::default(),
            ),
        ] {
            let (change, actual_version) = execute_with_outcome(patch, outcome);

            assert_eq!(change.viewer_rows_changed, viewer_rows_changed);
            assert_eq!(actual_version, version);
        }
    }
}
