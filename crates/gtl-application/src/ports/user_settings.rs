use std::path::{Path, PathBuf};

use gtl_models::settings::UserSettings;

use crate::settings::UserSettingsPatch;

/// A readable settings file did not represent a supported configuration.
#[derive(Debug, thiserror::Error)]
#[error("user settings at {} are invalid: {source}", path.display())]
pub struct UserSettingsConfigurationError {
    path: PathBuf,
    #[source]
    source: anyhow::Error,
}

impl UserSettingsConfigurationError {
    /// Associates a concrete document failure with its settings path.
    #[must_use]
    pub fn new(path: PathBuf, source: anyhow::Error) -> Self {
        Self { path, source }
    }

    /// Returns the invalid settings path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A strict user-settings snapshot could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum UserSettingsLoadError {
    /// The file was readable but was not a valid supported settings document.
    #[error(transparent)]
    InvalidConfiguration(#[from] UserSettingsConfigurationError),
    /// The settings adapter failed outside the document contract.
    #[error(transparent)]
    Adapter(#[from] anyhow::Error),
}

/// A settings edit conflicted with another writer.
#[derive(Debug, thiserror::Error)]
pub enum UserSettingsEditConflict {
    /// The caller based its edit on an older serialized settings document.
    #[error("user settings at {} changed after they were loaded", path.display())]
    StaleRevision { path: PathBuf },
    /// Another editor held the settings lease for the complete wait budget.
    #[error("user-settings lock {} was not acquired within {wait_seconds} seconds", path.display())]
    LockTimeout { path: PathBuf, wait_seconds: u64 },
    /// The document changed after it was read and before it could be replaced.
    #[error("user settings at {} changed while the edit was in progress", path.display())]
    ConcurrentModification { path: PathBuf },
}

/// A strict user-settings edit could not be completed safely.
#[derive(Debug, thiserror::Error)]
pub enum UserSettingsEditError {
    /// The existing or proposed document was not a valid supported configuration.
    #[error(transparent)]
    InvalidConfiguration(#[from] UserSettingsConfigurationError),
    /// Another writer prevented the edit from completing safely.
    #[error(transparent)]
    Conflict(#[from] UserSettingsEditConflict),
    /// The settings adapter failed outside the document contract.
    #[error(transparent)]
    Adapter(#[from] anyhow::Error),
}

/// Whether a user-settings edit changed the persisted document.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UserSettingsEditOutcome {
    /// The persisted document changed.
    Changed,
    /// The patch had no persistent effect.
    Unchanged,
}

impl UserSettingsEditOutcome {
    pub(crate) const fn changed(self) -> bool {
        matches!(self, Self::Changed)
    }
}

/// Loads validated user-settings snapshots.
pub trait UserSettingsReader: Clone + Send + Sync + 'static {
    /// Loads the latest complete settings snapshot.
    fn load(&self) -> Result<UserSettings, UserSettingsLoadError>;
}

/// Performs serialized edits after providing the corresponding read capability.
pub trait UserSettingsEditor: UserSettingsReader {
    /// Applies one complete typed settings patch atomically.
    ///
    /// # Errors
    ///
    /// Returns a semantic configuration or conflict error when the edit is rejected, or an
    /// adapter error when persistence cannot complete.
    fn edit(
        &mut self,
        patch: UserSettingsPatch,
    ) -> Result<UserSettingsEditOutcome, UserSettingsEditError>;
}

/// Recovery preserves the observed invalid document before restoring defaults.
pub trait UserSettingsRecovery: UserSettingsReader {
    fn inspect(&self) -> Result<UserSettingsRecoveryState, UserSettingsLoadError>;

    fn reset_invalid(
        &mut self,
        revision: &str,
        timestamp: &gtl_models::timestamps::MachineTimestamp,
    ) -> Result<PathBuf, UserSettingsEditError>;
}

#[derive(Debug, Clone)]
pub struct UserSettingsRecoveryState {
    pub configuration_path: PathBuf,
    pub diagnostic: Option<String>,
    pub revision: String,
}
