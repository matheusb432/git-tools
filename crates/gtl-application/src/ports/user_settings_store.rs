use std::{io, path::PathBuf};

use gtl_models::settings::UserSettings;

/// A strict user-settings snapshot could not be loaded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UserSettingsLoadError {
    /// The configured file could not be read from the filesystem.
    #[error("could not read user settings at {}: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The file was readable but was not a valid supported settings document.
    #[error("user settings at {} are invalid: {reason}", path.display())]
    InvalidConfiguration { path: PathBuf, reason: String },
}

/// A strict user-settings edit could not be completed safely.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UserSettingsEditError {
    /// The selected root item exists but is not a string.
    #[error("the existing user setting is not a string")]
    InvalidValueShape,
    /// Another editor held the settings lease for the complete wait budget.
    #[error("user-settings lock {} was not acquired within {wait_seconds} seconds", path.display())]
    LockTimeout { path: PathBuf, wait_seconds: u64 },
    /// The document changed after it was read and before it could be replaced.
    #[error("user settings at {} changed while the edit was in progress", path.display())]
    ConcurrentModification { path: PathBuf },
    /// The existing or proposed document was not a valid supported configuration.
    #[error("user settings at {} are invalid: {reason}", path.display())]
    InvalidConfiguration { path: PathBuf, reason: String },
    /// An unexpected filesystem or transaction effect failed.
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Loads validated settings snapshots and performs serialized strict edits.
pub trait UserSettingsStore: Clone + Send + Sync + 'static {
    /// Loads the latest complete settings snapshot.
    fn load(&self) -> Result<UserSettings, UserSettingsLoadError>;

    /// Replaces one root string and returns its previous value.
    fn set_string(
        &mut self,
        key: &str,
        value_new: &str,
    ) -> Result<Option<String>, UserSettingsEditError>;

    /// Removes one root string and returns its previous value.
    fn remove_string(&mut self, key: &str) -> Result<Option<String>, UserSettingsEditError>;
}
