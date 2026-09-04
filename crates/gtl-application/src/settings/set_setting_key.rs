use gtl_models::settings::SettingKeyValue;
use thiserror::Error;

use super::{UserSettingChange, setting_changes_viewer_rows};
use crate::{
    ports::{UserSettingsEditError, UserSettingsEditor},
    viewer::{ViewerState, ViewerStateError},
};

/// Reports a rejected or failed setting replacement.
#[derive(Debug, Error)]
pub enum SetSettingKeyError {
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
    #[error(transparent)]
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
    let key = mutation.key();
    let outcome = settings_editor.edit(mutation.into())?;
    if outcome.changed() {
        viewer_state.mark_shell_changed()?;
    }

    Ok(UserSettingChange {
        viewer_rows_changed: outcome.changed() && setting_changes_viewer_rows(key),
    })
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
                SettingKeyValue::Theme(Theme::Light),
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
