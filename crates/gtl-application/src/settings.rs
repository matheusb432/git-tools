//! Application operations for validated user-settings edits.

use crate::{
    ports::{UserSettingsEditError, UserSettingsEditor},
    viewer::{ViewerState, ViewerStateError},
};

pub mod edit_settings;
pub mod get_user_settings;
pub mod remove_setting_key;
pub mod reset_settings;
pub mod set_setting_key;
mod user_settings_patch;

pub use user_settings_patch::{UserSettingsFieldUpdate, UserSettingsPatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserSettingChange {
    pub viewer_rows_changed: bool,
}

fn apply_settings_patch<Error>(
    patch: UserSettingsPatch,
    settings_editor: &mut impl UserSettingsEditor,
    viewer_state: &ViewerState,
) -> Result<UserSettingChange, Error>
where
    Error: From<UserSettingsEditError> + From<ViewerStateError>,
{
    let viewer_rows_selected = patch.changes_viewer_rows();
    let outcome = settings_editor.edit(patch)?;
    if outcome.changed() {
        viewer_state.mark_shell_changed()?;
    }
    Ok(UserSettingChange {
        viewer_rows_changed: outcome.changed() && viewer_rows_selected,
    })
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
            Ok(UserSettings::default())
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
