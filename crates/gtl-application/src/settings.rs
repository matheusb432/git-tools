//! Application operations for validated user-settings edits.

use gtl_models::settings::SettingKey;

pub mod edit_settings;
pub mod get_user_settings;
pub mod remove_setting_key;
pub mod set_setting_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserSettingChange {
    pub viewer_rows_changed: bool,
}

const fn setting_changes_viewer_rows(key: SettingKey) -> bool {
    matches!(key, SettingKey::Layout | SettingKey::Density)
}

#[cfg(test)]
mod test_support {
    use gtl_models::settings::{SettingKey, SettingKeyValue, UserSettings};

    use crate::ports::{
        UserSettingsEditError, UserSettingsEditOutcome, UserSettingsLoadError, UserSettingsStore,
    };

    #[derive(Clone)]
    pub(super) struct FixedUserSettingsEditStore {
        outcome: UserSettingsEditOutcome,
    }

    impl FixedUserSettingsEditStore {
        pub(super) const fn new(outcome: UserSettingsEditOutcome) -> Self {
            Self { outcome }
        }
    }

    impl UserSettingsStore for FixedUserSettingsEditStore {
        fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
            Ok(crate::utils::default_user_settings())
        }

        fn set_value(
            &mut self,
            _mutation: SettingKeyValue,
        ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
            Ok(self.outcome)
        }

        fn remove_key(
            &mut self,
            _key: SettingKey,
        ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
            Ok(self.outcome)
        }
    }
}
