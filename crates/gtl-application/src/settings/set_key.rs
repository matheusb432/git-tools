use gtl_models::settings::{SettingKeyValue, SettingKeyValueError};
use thiserror::Error;

use crate::ports::{UserSettingsEditError, UserSettingsStore};

/// Requests one validated root-string setting replacement.
#[derive(Debug, Eq, PartialEq)]
pub struct SetSettingKey {
    pub key: String,
    pub value_new: String,
    // TODO: make this be the command param, refactor call sites. much cleaner this way.
    // pub key_value: SettingKeyValue,
}

/// Describes the completed replacement and its previous value.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     ports::UserSettingsStore,
///     settings::set_key::{self, SetSettingKey, SetSettingKeyError},
/// };
///
/// fn previous_theme(
///     store: &mut impl UserSettingsStore,
/// ) -> Result<Option<String>, SetSettingKeyError> {
///     Ok(set_key::execute(
///         SetSettingKey {
///             key: "theme".into(),
///             value_new: "light".into(),
///         },
///         store,
///     )?
///     .value_old)
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SetSettingKeyOk {
    pub key: String,
    pub value_old: Option<String>,
    pub value_new: String,
}

/// Reports a rejected or failed setting replacement.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SetSettingKeyError {
    // TODO: move this somewhere else, post refactor
    #[error(transparent)]
    InvalidSettingKey(#[from] SettingKeyValueError),
    #[error("`{value}` is not a valid value for user setting `{key}`")]
    InvalidValue { key: String, value: String },
    #[error("user setting `{key}` must be a string")]
    InvalidValueShape { key: String },
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
}

/// Validates and sets one supported scalar user setting.
#[cqrsy::command]
pub fn execute(
    command: SetSettingKey,
    settings_store: &mut impl UserSettingsStore,
) -> Result<SetSettingKeyOk, SetSettingKeyError> {
    let SetSettingKey { key, value_new } = command;
    let _setting_key_value = SettingKeyValue::new(&key, &value_new)?;
    // TODO: use '_setting_key_value' value post refactor
    let value_old = settings_store
        .set_string(&key, &value_new)
        .map_err(|error| match error {
            UserSettingsEditError::InvalidValueShape => {
                SetSettingKeyError::InvalidValueShape { key: key.clone() }
            }
            error => SetSettingKeyError::Settings(error),
        })?;

    Ok(SetSettingKeyOk {
        key,
        value_old,
        value_new,
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::settings::UserSettings;

    use super::{SetSettingKey, SetSettingKeyError, execute};
    use crate::ports::{UserSettingsEditError, UserSettingsLoadError, UserSettingsStore};

    #[derive(Clone)]
    struct InvalidShapeSettingsStore;

    impl UserSettingsStore for InvalidShapeSettingsStore {
        fn load(&self) -> Result<UserSettings, UserSettingsLoadError> {
            Err(UserSettingsLoadError::InvalidConfiguration {
                path: "<unused>".into(),
                reason: "set_key does not load settings".into(),
            })
        }

        fn set_string(
            &mut self,
            _key: &str,
            _value_new: &str,
        ) -> Result<Option<String>, UserSettingsEditError> {
            Err(UserSettingsEditError::InvalidValueShape)
        }

        fn remove_string(&mut self, _key: &str) -> Result<Option<String>, UserSettingsEditError> {
            Err(UserSettingsEditError::InvalidValueShape)
        }
    }

    #[test]
    fn execute_adds_the_setting_key_to_an_invalid_shape_error() {
        let error = execute(
            SetSettingKey {
                key: "theme".into(),
                value_new: "light".into(),
            },
            &mut InvalidShapeSettingsStore,
        )
        .expect_err("invalid existing shape must fail");

        assert!(matches!(
            error,
            SetSettingKeyError::InvalidValueShape { key } if key == "theme"
        ));
    }
}
