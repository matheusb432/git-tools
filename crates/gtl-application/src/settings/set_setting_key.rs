use gtl_models::settings::{SettingKey, SettingKeyValue};
use thiserror::Error;

use crate::ports::{UserSettingsEditError, UserSettingsStore};

/// Requests one validated root-string setting replacement.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SetSettingKey {
    pub mutation: SettingKeyValue,
}

/// Describes the completed replacement and its previous value.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     ports::UserSettingsStore,
///     settings::set_setting_key::{self, SetSettingKey, SetSettingKeyError},
/// };
/// use gtl_models::{settings::SettingKeyValue, viewer::Theme};
///
/// fn previous_theme(
///     store: &mut impl UserSettingsStore,
/// ) -> Result<Option<String>, SetSettingKeyError> {
///     Ok(set_setting_key::execute(
///         SetSettingKey {
///             mutation: SettingKeyValue::Theme(Theme::Light),
///         },
///         store,
///     )?
///     .value_old)
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SetSettingKeyOk {
    pub key: SettingKey,
    pub value_old: Option<String>,
    pub value_new: SettingKeyValue,
}

/// Reports a rejected or failed setting replacement.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SetSettingKeyError {
    #[error("user setting `{key}` must be a string")]
    InvalidValueShape { key: SettingKey },
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
}

/// Validates and sets one supported scalar user setting.
#[cqrsy::command]
pub fn execute(
    command: SetSettingKey,
    settings_store: &mut impl UserSettingsStore,
) -> Result<SetSettingKeyOk, SetSettingKeyError> {
    let SetSettingKey { mutation } = command;
    let key = mutation.key();
    let value_old = settings_store
        .set_value(mutation)
        .map_err(|error| match error {
            UserSettingsEditError::InvalidValueShape => {
                SetSettingKeyError::InvalidValueShape { key }
            }
            error => SetSettingKeyError::Settings(error),
        })?;

    Ok(SetSettingKeyOk {
        key,
        value_old,
        value_new: mutation,
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        settings::{SettingKey, SettingKeyValue, UserSettings},
        viewer::Theme,
    };

    use super::{SetSettingKey, SetSettingKeyError};
    use crate::{
        ports::{UserSettingsEditError, UserSettingsLoadError, UserSettingsStore},
        settings::set_setting_key,
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
        ) -> Result<Option<String>, UserSettingsEditError> {
            Err(UserSettingsEditError::InvalidValueShape)
        }

        fn remove_key(
            &mut self,
            _key: SettingKey,
        ) -> Result<Option<String>, UserSettingsEditError> {
            Err(UserSettingsEditError::InvalidValueShape)
        }
    }

    #[test]
    fn execute_adds_the_setting_key_to_an_invalid_shape_error() {
        let error = set_setting_key::execute(
            SetSettingKey {
                mutation: SettingKeyValue::Theme(Theme::Light),
            },
            &mut InvalidShapeSettingsStore,
        )
        .expect_err("invalid existing shape must fail");

        assert!(matches!(
            error,
            SetSettingKeyError::InvalidValueShape { key } if key == SettingKey::Theme
        ));
    }
}
