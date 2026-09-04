use gtl_models::settings::SettingKey;
use thiserror::Error;

use super::{UserSettingChange, setting_changes_viewer_rows};
use crate::{
    ports::{UserSettingsEditError, UserSettingsEditor},
    viewer::{ViewerState, ViewerStateError},
};

/// Reports a rejected or failed setting removal.
#[derive(Debug, Error)]
pub enum RemoveSettingKeyError {
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
    #[error(transparent)]
    ViewerState(#[from] ViewerStateError),
}

/// Removes one supported scalar user setting so its default applies.
///
/// # Errors
///
/// Validation and persistence failures leave the settings document unchanged. A viewer-state
/// failure can be returned after the setting was removed.
#[cqrsy::command]
pub fn execute(
    key: SettingKey,
    settings_editor: &mut impl UserSettingsEditor,
    viewer_state: &ViewerState,
) -> Result<UserSettingChange, RemoveSettingKeyError> {
    let outcome = settings_editor.edit(super::UserSettingsPatch::clear(key))?;
    if outcome.changed() {
        viewer_state.mark_shell_changed()?;
    }

    Ok(UserSettingChange {
        viewer_rows_changed: outcome.changed() && setting_changes_viewer_rows(key),
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::{settings::SettingKey, viewer::ViewerVersion};

    use crate::{
        ports::UserSettingsEditOutcome,
        settings::{remove_setting_key, test_support::FixedUserSettingsEditStore},
        viewer::ViewerState,
    };

    fn execute_with_outcome(
        key: SettingKey,
        outcome: UserSettingsEditOutcome,
    ) -> (crate::settings::UserSettingChange, ViewerVersion) {
        let mut store = FixedUserSettingsEditStore::new(outcome);
        let viewer = ViewerState::new();
        let response = remove_setting_key::execute(key, &mut store, &viewer).unwrap();
        let version = viewer.version().unwrap();
        (response, version)
    }

    #[test]
    fn successful_removals_publish_only_actual_viewer_changes() {
        for (key, outcome, viewer_rows_changed, version) in [
            (
                SettingKey::Theme,
                UserSettingsEditOutcome::Changed,
                false,
                ViewerVersion::new(1),
            ),
            (
                SettingKey::Layout,
                UserSettingsEditOutcome::Changed,
                true,
                ViewerVersion::new(1),
            ),
            (
                SettingKey::Density,
                UserSettingsEditOutcome::Unchanged,
                false,
                ViewerVersion::default(),
            ),
        ] {
            let (response, actual_version) = execute_with_outcome(key, outcome);

            assert_eq!(response.viewer_rows_changed, viewer_rows_changed);
            assert_eq!(actual_version, version);
        }
    }
}
