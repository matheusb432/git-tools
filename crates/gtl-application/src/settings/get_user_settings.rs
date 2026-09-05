use gtl_models::settings::UserSettings;

use crate::ports::{UserSettingsLoadError, UserSettingsReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetUserSettings;

#[derive(Debug, thiserror::Error)]
pub enum GetUserSettingsError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
}

#[cqrsy::query]
pub fn execute(
    _query: GetUserSettings,
    settings_reader: &impl UserSettingsReader,
) -> Result<UserSettings, GetUserSettingsError> {
    Ok(settings_reader.load()?)
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::DiffExclusions,
        settings::UserSettings,
        viewer::{DiffDensity, DiffLayout, RenderOptions, Theme},
    };

    use super::{GetUserSettings, GetUserSettingsError};
    use crate::{
        ports::UserSettingsLoadError,
        settings::get_user_settings,
        utils::{FixedUserSettingsStore, SequenceUserSettingsStore},
    };

    #[test]
    fn query_returns_the_complete_validated_settings_snapshot() {
        let settings = UserSettings::new(
            Some(Theme::Hearth),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            gtl_models::viewer::ViewerKeybindings::default(),
            false,
            DiffExclusions::new(
                [(crate::utils::project_name("git-tools"), vec!["js"])],
                Some(vec!["md"]),
            ),
            gtl_models::settings::PushAllExclusions::default(),
        );
        let store = FixedUserSettingsStore::new(settings.clone());

        let response = get_user_settings::execute(GetUserSettings, &store).unwrap();

        assert_eq!(response, settings);
    }

    #[test]
    fn query_preserves_a_typed_settings_adapter_failure() {
        let store = SequenceUserSettingsStore::new([]);

        let error = get_user_settings::execute(GetUserSettings, &store).unwrap_err();

        assert!(matches!(
            error,
            GetUserSettingsError::Settings(UserSettingsLoadError::Adapter(_))
        ));
    }
}
