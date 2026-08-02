use super::UserSettingsStore;

/// Reports a strict user-settings document edit failure.
///
/// # Examples
///
/// ```
/// use gtl_application::ports::UserSettingsEditError;
///
/// let error = UserSettingsEditError::InvalidValueShape;
/// assert!(error.to_string().contains("not a string"));
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UserSettingsEditError {
    /// The existing root item is present but is not a string.
    #[error("the existing user setting is not a string")]
    InvalidValueShape,
    /// A path, read, parse, lock, synchronization, or replacement effect failed.
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Edits root string values in the configured user-settings document.
///
/// # Examples
///
/// ```no_run
/// use gtl_application::ports::{UserSettingsEditError, UserSettingsEditor};
///
/// fn select_light_theme(
///     store: &impl UserSettingsEditor,
/// ) -> Result<Option<String>, UserSettingsEditError> {
///     store.set_string("theme", "light")
/// }
/// ```
pub trait UserSettingsEditor: UserSettingsStore {
    /// Sets one root string and returns the previous string.
    ///
    /// # Errors
    ///
    /// Returns [`UserSettingsEditError`] when the existing item is not a string or
    /// the strict file transaction fails.
    fn set_string(
        &self,
        key: &str,
        value_new: &str,
    ) -> Result<Option<String>, UserSettingsEditError>;

    /// Removes one root string and returns the removed string.
    ///
    /// # Errors
    ///
    /// Returns [`UserSettingsEditError`] when the existing item is not a string or
    /// the strict file transaction fails.
    fn remove_string(&self, key: &str) -> Result<Option<String>, UserSettingsEditError>;
}
