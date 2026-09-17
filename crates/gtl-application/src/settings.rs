//! Application operations for validated user-settings edits.

use gtl_models::settings::SettingKey;

pub mod edit_settings;
pub mod get_user_settings;
pub mod remove_setting_key;
pub mod reset_settings;
pub mod set_setting_key;
mod user_settings_patch;

pub use user_settings_patch::{
    DiffExclusionsUpdate, DuplicateProjectSettingsNameError, ProjectSettingsUpdate,
    ProjectSettingsUpdates, UserSettingsFieldUpdate, UserSettingsPatch,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserSettingChange {
    pub viewer_rows_changed: bool,
}

const fn setting_changes_viewer_rows(key: SettingKey) -> bool {
    matches!(key, SettingKey::Layout | SettingKey::Density)
}

#[cfg(test)]
mod test_support {
    use gtl_models::settings::UserSettings;

    use crate::ports::{
        UserSettingsEditError, UserSettingsEditOutcome, UserSettingsEditor, UserSettingsLoadError,
        UserSettingsReader,
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

    impl UserSettingsReader for FixedUserSettingsEditStore {
        fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
            Ok(crate::utils::default_user_settings())
        }
    }

    impl UserSettingsEditor for FixedUserSettingsEditStore {
        fn edit(
            &mut self,
            _patch: super::UserSettingsPatch,
        ) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
            Ok(self.outcome)
        }
    }
}
