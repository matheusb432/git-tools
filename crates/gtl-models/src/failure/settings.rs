use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{ErrorClass, ExternalDiagnostic, Failure, PublicFailure};
use crate::paths::ProjectName;

/// Why user settings cannot be read or changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsFailure {
    /// The settings file cannot be parsed or violates the settings schema.
    Invalid {
        path: PathBuf,
        diagnostic: ExternalDiagnostic,
    },
    /// The settings changed after the caller loaded them or while the edit ran.
    Stale,
    /// Another editor held the settings lock for the whole wait budget.
    Locked { wait_seconds: u64 },
    /// The server has no user configuration path to write.
    PathUnavailable,
    /// A project-settings replacement names the same project more than once.
    DuplicateProject { name: ProjectName },
}

impl SettingsFailure {
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::Invalid { .. } | Self::PathUnavailable => ErrorClass::FailedPrecondition,
            Self::Stale | Self::Locked { .. } => ErrorClass::Aborted,
            Self::DuplicateProject { .. } => ErrorClass::InvalidArgument,
        }
    }
}

impl PublicFailure for SettingsFailure {
    fn failure(&self) -> Failure {
        Failure::Settings(self.clone())
    }
}

impl From<SettingsFailure> for Failure {
    fn from(failure: SettingsFailure) -> Self {
        Self::Settings(failure)
    }
}

impl std::error::Error for SettingsFailure {}

impl fmt::Display for SettingsFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { path, .. } => write!(
                formatter,
                "User settings at {} are invalid. Repair the settings file or back it up and reset it.",
                path.display()
            ),
            Self::Stale => formatter.write_str(
                "Settings changed since they were loaded. Reload them before saving again.",
            ),
            Self::Locked { wait_seconds } => write!(
                formatter,
                "Another editor held the settings lock for {wait_seconds} seconds. Try again."
            ),
            Self::PathUnavailable => {
                formatter.write_str("The user configuration path is unavailable.")
            }
            Self::DuplicateProject { name } => write!(
                formatter,
                "The project settings name {name} more than once."
            ),
        }
    }
}
