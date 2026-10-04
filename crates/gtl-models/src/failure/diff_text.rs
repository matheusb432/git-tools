use std::fmt;

use serde::{Deserialize, Serialize};

use super::{ErrorClass, ExternalDiagnostic, Failure, PublicFailure};

/// Why diff text supplied outside a repository cannot be stored or shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffTextFailure {
    /// The text exceeds the accepted size.
    TooLarge { bytes_max: u64 },
    /// The text has no `diff --git` file section.
    NoFiles,
    /// A `diff --git` header names a path that cannot be read.
    InvalidFileHeader { diagnostic: ExternalDiagnostic },
    /// Two file sections name the same path.
    DuplicatePath { path: String },
    /// The stored text was removed after its last viewer tab and history entry closed.
    Missing,
}

impl DiffTextFailure {
    /// The rejected header and the reason it cannot be read.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::InvalidFileHeader { diagnostic } => Some(diagnostic),
            _ => None,
        }
    }

    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::TooLarge { .. } => ErrorClass::ResourceExhausted,
            Self::NoFiles | Self::InvalidFileHeader { .. } | Self::DuplicatePath { .. } => {
                ErrorClass::InvalidArgument
            }
            Self::Missing => ErrorClass::NotFound,
        }
    }
}

impl PublicFailure for DiffTextFailure {
    fn failure(&self) -> Failure {
        Failure::DiffText(self.clone())
    }
}

impl From<DiffTextFailure> for Failure {
    fn from(failure: DiffTextFailure) -> Self {
        Self::DiffText(failure)
    }
}

impl std::error::Error for DiffTextFailure {}

impl fmt::Display for DiffTextFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes_max } => write!(
                formatter,
                "The diff text is larger than {} MiB.",
                bytes_max / (1024 * 1024)
            ),
            Self::NoFiles => {
                formatter.write_str("The diff text has no `diff --git` file sections.")
            }
            Self::InvalidFileHeader { .. } => {
                formatter.write_str("The diff text has a file header that cannot be read.")
            }
            Self::DuplicatePath { path } => {
                write!(formatter, "The diff text lists {path} more than once.")
            }
            Self::Missing => formatter.write_str("This diff text is no longer stored."),
        }
    }
}
