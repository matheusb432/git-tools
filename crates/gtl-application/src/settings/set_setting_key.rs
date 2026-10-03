use gtl_models::{failure::ErrorMeta, settings::SettingKeyValue};
use thiserror::Error;

use super::{UserSettingChange, apply_settings_patch};
use crate::{
    ports::{UserSettingsEditError, UserSettingsEditor},
    viewer::{ViewerState, ViewerStateError},
};

/// Reports a rejected or failed setting replacement.
#[derive(Debug, Error, ErrorMeta)]
pub enum SetSettingKeyError {
    #[error(transparent)]
    #[meta(transparent)]
    Settings(#[from] UserSettingsEditError),
    #[error(transparent)]
    #[meta(transparent)]
    ViewerState(#[from] ViewerStateError),
}

/// Sets one scalar user setting and publishes actual changes to viewer watchers.
///
/// # Errors
///
/// Validation and persistence failures leave the settings document unchanged. A viewer-state
/// failure can be returned after the setting was persisted.
#[cqrsy::command]
pub fn execute(
    mutation: SettingKeyValue,
    settings_editor: &mut impl UserSettingsEditor,
    viewer_state: &ViewerState,
) -> Result<UserSettingChange, SetSettingKeyError> {
    apply_settings_patch(mutation.into(), settings_editor, viewer_state)
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        settings::SettingKeyValue,
        viewer::{DiffDensity, DiffLayout, Theme, ViewerVersion},
    };

    use crate::{
        ports::UserSettingsEditOutcome,
        settings::{set_setting_key, test_support::FixedUserSettingsEditStore},
        viewer::ViewerState,
    };

    fn execute_with_outcome(
        mutation: SettingKeyValue,
        outcome: UserSettingsEditOutcome,
    ) -> (crate::settings::UserSettingChange, ViewerVersion) {
        let mut store = FixedUserSettingsEditStore::new(outcome);
        let viewer = ViewerState::new();
        let response = set_setting_key::execute(mutation, &mut store, &viewer).unwrap();
        let version = viewer.version().unwrap();
        (response, version)
    }

    #[test]
    fn successful_edits_publish_only_actual_viewer_changes() {
        for (mutation, outcome, viewer_rows_changed, version) in [
            (
                SettingKeyValue::Theme(Theme::Glacier),
                UserSettingsEditOutcome::Changed,
                false,
                ViewerVersion::new(1),
            ),
            (
                SettingKeyValue::Layout(DiffLayout::Split),
                UserSettingsEditOutcome::Changed,
                true,
                ViewerVersion::new(1),
            ),
            (
                SettingKeyValue::Density(DiffDensity::Compact),
                UserSettingsEditOutcome::Unchanged,
                false,
                ViewerVersion::default(),
            ),
        ] {
            let (response, actual_version) = execute_with_outcome(mutation, outcome);

            assert_eq!(response.viewer_rows_changed, viewer_rows_changed);
            assert_eq!(actual_version, version);
        }
    }
}
