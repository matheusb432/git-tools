use thiserror::Error;

use crate::ports::{UserSettingsEditError, UserSettingsStore};

/// Requests removal of one supported root-string setting.
#[derive(Debug, Eq, PartialEq)]
pub struct RemoveSettingKey {
    pub key: String,
}

/// Describes the completed removal and its previous value.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     ports::UserSettingsStore,
///     settings::remove_key::{self, RemoveSettingKey, RemoveSettingKeyError},
/// };
///
/// fn previous_layout(
///     store: &mut impl UserSettingsStore,
/// ) -> Result<Option<String>, RemoveSettingKeyError> {
///     Ok(remove_key::execute(
///         RemoveSettingKey {
///             key: "layout".into(),
///         },
///         store,
///     )?
///     .value_old)
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RemoveSettingKeyOk {
    pub key: String,
    pub value_old: Option<String>,
}

/// Reports a rejected or failed setting removal.
///
/// # Examples
///
/// ```
/// use gtl_application::settings::remove_key::RemoveSettingKeyError;
///
/// let error = RemoveSettingKeyError::InvalidKey {
///     key: "nested".into(),
/// };
/// assert!(error.to_string().contains("nested"));
/// ```
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RemoveSettingKeyError {
    #[error("the key `{key}` is not a supported user setting")]
    InvalidKey { key: String },
    #[error("user setting `{key}` must be a string")]
    InvalidValueShape { key: String },
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
}

fn validate(key: &str) -> Result<(), RemoveSettingKeyError> {
    if matches!(key, "theme" | "layout" | "density") {
        Ok(())
    } else {
        Err(RemoveSettingKeyError::InvalidKey {
            key: key.to_owned(),
        })
    }
}

/// Removes one supported scalar user setting so its default applies.
///
/// # Errors
///
/// Returns [`RemoveSettingKeyError`] without changing the target document when
/// validation or the store transaction fails.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     ports::UserSettingsStore,
///     settings::remove_key::{self, RemoveSettingKey, RemoveSettingKeyError},
/// };
///
/// fn remove_density(store: &mut impl UserSettingsStore) -> Result<(), RemoveSettingKeyError> {
///     remove_key::execute(
///         RemoveSettingKey {
///             key: "density".into(),
///         },
///         store,
///     )?;
///     Ok(())
/// }
/// ```
#[cqrsy::command]
pub fn execute(
    command: RemoveSettingKey,
    settings_store: &mut impl UserSettingsStore,
) -> Result<RemoveSettingKeyOk, RemoveSettingKeyError> {
    let RemoveSettingKey { key } = command;
    validate(&key)?;
    let value_old = settings_store
        .remove_string(&key)
        .map_err(|error| match error {
            UserSettingsEditError::InvalidValueShape => {
                RemoveSettingKeyError::InvalidValueShape { key: key.clone() }
            }
            error => RemoveSettingKeyError::Settings(error),
        })?;

    Ok(RemoveSettingKeyOk { key, value_old })
}

#[cfg(test)]
mod tests {
    use super::{RemoveSettingKeyError, validate};

    #[test]
    fn validation_accepts_only_supported_scalar_keys() {
        for key in ["theme", "layout", "density"] {
            validate(key).expect("supported key");
        }
        assert!(matches!(
            validate("push.confirm"),
            Err(RemoveSettingKeyError::InvalidKey { key }) if key == "push.confirm"
        ));
    }
}
