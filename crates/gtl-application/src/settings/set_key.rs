use std::str::FromStr as _;

use gtl_models::viewer::{DiffDensity, DiffLayout, Theme};
use thiserror::Error;

use crate::ports::{UserSettingsEditError, UserSettingsEditor};

/// Requests one validated root-string setting replacement.
#[derive(Debug, Eq, PartialEq)]
pub struct SetSettingKey {
    pub key: String,
    pub value_new: String,
}

/// Describes the completed replacement and its previous value.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     ports::UserSettingsEditor,
///     settings::set_key::{self, SetSettingKey, SetSettingKeyError},
/// };
///
/// fn previous_theme(
///     store: &impl UserSettingsEditor,
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
///
/// # Examples
///
/// ```
/// use gtl_application::settings::set_key::SetSettingKeyError;
///
/// let error = SetSettingKeyError::InvalidKey {
///     key: "nested".into(),
/// };
/// assert!(error.to_string().contains("nested"));
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SetSettingKeyError {
    #[error("the key `{key}` is not a supported user setting")]
    InvalidKey { key: String },
    #[error("`{value}` is not a valid value for user setting `{key}`")]
    InvalidValue { key: String, value: String },
    #[error("user setting `{key}` must be a string")]
    InvalidValueShape { key: String },
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

fn validate(key: &str, value_new: &str) -> Result<(), SetSettingKeyError> {
    let value_valid = match key {
        "theme" => Theme::from_str(value_new).is_ok(),
        "layout" => DiffLayout::from_str(value_new).is_ok(),
        "density" => DiffDensity::from_str(value_new).is_ok(),
        _ => {
            return Err(SetSettingKeyError::InvalidKey {
                key: key.to_owned(),
            });
        }
    };
    if !value_valid {
        return Err(SetSettingKeyError::InvalidValue {
            key: key.to_owned(),
            value: value_new.to_owned(),
        });
    }
    Ok(())
}

/// Validates and sets one supported scalar user setting.
///
/// # Errors
///
/// Returns [`SetSettingKeyError`] without changing the target document when
/// validation or the store transaction fails.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     ports::UserSettingsEditor,
///     settings::set_key::{self, SetSettingKey, SetSettingKeyError},
/// };
///
/// fn set_layout(store: &impl UserSettingsEditor) -> Result<(), SetSettingKeyError> {
///     set_key::execute(
///         SetSettingKey {
///             key: "layout".into(),
///             value_new: "split".into(),
///         },
///         store,
///     )?;
///     Ok(())
/// }
/// ```
#[cqrsy::command]
pub fn execute(
    command: SetSettingKey,
    settings_store: &impl UserSettingsEditor,
) -> Result<SetSettingKeyOk, SetSettingKeyError> {
    let SetSettingKey { key, value_new } = command;
    validate(&key, &value_new)?;
    let value_old = settings_store
        .set_string(&key, &value_new)
        .map_err(|error| match error {
            UserSettingsEditError::InvalidValueShape => {
                SetSettingKeyError::InvalidValueShape { key: key.clone() }
            }
            UserSettingsEditError::Unexpected(error) => {
                SetSettingKeyError::Unexpected(error.context(format!("set user setting `{key}`")))
            }
        })?;

    Ok(SetSettingKeyOk {
        key,
        value_old,
        value_new,
    })
}

#[cfg(test)]
mod tests {
    use super::{SetSettingKeyError, validate};

    #[test]
    fn validation_rejects_unknown_keys_and_values() {
        assert!(matches!(
            validate("nested", "value"),
            Err(SetSettingKeyError::InvalidKey { key }) if key == "nested"
        ));
        assert!(matches!(
            validate("theme", "blue"),
            Err(SetSettingKeyError::InvalidValue { key, value })
                if key == "theme" && value == "blue"
        ));
    }

    #[test]
    fn validation_accepts_every_supported_value() {
        use strum::VariantArray as _;

        let theme_tokens: Vec<String> = gtl_models::viewer::Theme::VARIANTS
            .iter()
            .map(ToString::to_string)
            .collect();
        let supported: [(&str, Vec<&str>); 3] = [
            ("theme", theme_tokens.iter().map(String::as_str).collect()),
            ("layout", vec!["unified", "split"]),
            ("density", vec!["compact", "full"]),
        ];

        for (key, values) in supported {
            for value in values {
                validate(key, value).expect("supported value");
            }
        }
    }
}
