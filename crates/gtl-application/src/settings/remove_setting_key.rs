use gtl_models::settings::SettingKey;
use thiserror::Error;

use crate::ports::{UserSettingsEditError, UserSettingsStore};

/// Requests removal of one supported root-string setting.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RemoveSettingKey {
    pub key: SettingKey,
}

/// Describes the completed removal and its previous value.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::{
///     ports::UserSettingsStore,
///     settings::remove_setting_key::{self, RemoveSettingKey, RemoveSettingKeyError},
/// };
/// use gtl_models::settings::SettingKey;
///
/// fn previous_layout(
///     store: &mut impl UserSettingsStore,
/// ) -> Result<Option<String>, RemoveSettingKeyError> {
///     Ok(remove_setting_key::execute(
///         RemoveSettingKey {
///             key: SettingKey::Layout,
///         },
///         store,
///     )?
///     .value_old)
/// }
/// ```
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RemoveSettingKeyOk {
    pub key: SettingKey,
    pub value_old: Option<String>,
}

/// Reports a rejected or failed setting removal.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RemoveSettingKeyError {
    #[error("user setting `{key}` must be a string")]
    InvalidValueShape { key: SettingKey },
    #[error(transparent)]
    Settings(#[from] UserSettingsEditError),
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
///     settings::remove_setting_key::{self, RemoveSettingKey, RemoveSettingKeyError},
/// };
/// use gtl_models::settings::SettingKey;
///
/// fn remove_density(store: &mut impl UserSettingsStore) -> Result<(), RemoveSettingKeyError> {
///     remove_setting_key::execute(
///         RemoveSettingKey {
///             key: SettingKey::Density,
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
    let value_old = settings_store
        .remove_key(key)
        .map_err(|error| match error {
            UserSettingsEditError::InvalidValueShape => {
                RemoveSettingKeyError::InvalidValueShape { key }
            }
            error => RemoveSettingKeyError::Settings(error),
        })?;

    Ok(RemoveSettingKeyOk { key, value_old })
}
