use gtl_models::settings::UserSettings;

use crate::ports::{UserSettingsLoadError, UserSettingsStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetUserSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetUserSettingsOk {
    pub settings: UserSettings,
}

#[derive(Debug, thiserror::Error)]
pub enum GetUserSettingsError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
}

#[cqrsy::query]
pub fn execute(
    _query: GetUserSettings,
    settings_store: &impl UserSettingsStore,
) -> Result<GetUserSettingsOk, GetUserSettingsError> {
    Ok(GetUserSettingsOk {
        settings: settings_store.load()?,
    })
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
            false,
            DiffExclusions::new(
                [
                    (crate::utils::project_name("defaults"), vec!["md"]),
                    (crate::utils::project_name("git-tools"), vec!["js"]),
                ],
                None,
            ),
        );
        let store = FixedUserSettingsStore::new(settings.clone());

        let response =
            get_user_settings::execute(GetUserSettings, &store).expect("settings query succeeds");

        assert_eq!(response.settings, settings);
    }

    #[test]
    fn query_preserves_a_typed_settings_load_failure() {
        let store = SequenceUserSettingsStore::new([]);

        let error =
            get_user_settings::execute(GetUserSettings, &store).expect_err("empty sequence fails");

        assert!(matches!(
            error,
            GetUserSettingsError::Settings(UserSettingsLoadError::InvalidConfiguration {
                path,
                ..
            }) if path == std::path::Path::new("<test settings sequence>")
        ));
    }
}
