use gtl_models::settings::{SettingKey, SettingKeyValue};
use thiserror::Error;

use super::{UserSettingChange, setting_changes_viewer_rows};
use crate::{
    ports::{UserSettingsEditError, UserSettingsStore},
    viewer::{ViewerState, ViewerStateError},
};

/// Reports a rejected or failed setting replacement.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SetSettingKeyError {
    #[error("user setting `{key}` must be a string")]
    InvalidValueShape { key: SettingKey },
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
    settings_store: &mut impl UserSettingsStore,
    viewer_state: &ViewerState,
) -> Result<UserSettingChange, SetSettingKeyError> {
    let key = mutation.key();
    let outcome = settings_store
        .set_value(mutation)
        .map_err(|error| match error {
            UserSettingsEditError::InvalidValueShape => {
                SetSettingKeyError::InvalidValueShape { key }
            }
            error => SetSettingKeyError::Settings(error),
        })?;
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
        settings::{SettingKey, SettingKeyValue, UserSettings},
        viewer::{DiffDensity, DiffLayout, Theme, ViewerVersion},
    };

    use super::SetSettingKeyError;
    use crate::{
        ports::{
            UserSettingsEditError, UserSettingsEditOutcome, UserSettingsLoadError,
            UserSettingsStore,
        },
        settings::{set_setting_key, test_support::FixedUserSettingsEditStore},
        viewer::ViewerState,
    };

    #[derive(Clone)]
    struct InvalidShapeSettingsStore;

    impl UserSettingsStore for InvalidShapeSettingsStore {
        fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
            Err(UserSettingsLoadError::InvalidConfiguration {
                path: "<unused>".into(),
                reason: "set_setting_key does not load settings".into(),
            })
        }

        fn set_value(
            &mut self,
            _mutation: SettingKeyValue,
        ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
            Err(UserSettingsEditError::InvalidValueShape)
        }

        fn remove_key(
            &mut self,
            _key: SettingKey,
        ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
            Err(UserSettingsEditError::InvalidValueShape)
        }
    }

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

    #[test]
    fn execute_adds_the_setting_key_to_an_invalid_shape_error() {
        let viewer = ViewerState::new();
        let error = set_setting_key::execute(
            SettingKeyValue::Theme(Theme::Light),
            &mut InvalidShapeSettingsStore,
            &viewer,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            SetSettingKeyError::InvalidValueShape { key } if key == SettingKey::Theme
        ));
        assert_eq!(viewer.version().unwrap(), ViewerVersion::default());
    }
}
