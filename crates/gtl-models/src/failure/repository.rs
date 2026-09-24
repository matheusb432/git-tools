use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{ErrorClass, ExternalDiagnostic, Failure, PublicFailure};

/// Why a repository path cannot be resolved or searched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryFailure {
    /// Git does not recognize the path as inside a repository.
    NotARepository { path: PathBuf },
    /// A recursive search found no repositories under the root.
    NoRepositories { root: PathBuf },
    /// The filesystem refused a recursive repository search.
    SearchFailed {
        path: PathBuf,
        diagnostic: ExternalDiagnostic,
    },
}

impl RepositoryFailure {
    /// Verbatim filesystem output that explains the failure.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::SearchFailed { diagnostic, .. } => Some(diagnostic),
            _ => None,
        }
    }

    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::NoRepositories { .. } => ErrorClass::NotFound,
            Self::NotARepository { .. } | Self::SearchFailed { .. } => {
                ErrorClass::FailedPrecondition
            }
        }
    }
}

impl PublicFailure for RepositoryFailure {
    fn failure(&self) -> Failure {
        Failure::Repository(self.clone())
    }
}

impl From<RepositoryFailure> for Failure {
    fn from(failure: RepositoryFailure) -> Self {
        Self::Repository(failure)
    }
}

impl std::error::Error for RepositoryFailure {}

impl fmt::Display for RepositoryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotARepository { path } => {
                write!(
                    formatter,
                    "{} is not inside a Git repository.",
                    path.display()
                )
            }
            Self::NoRepositories { root } => write!(
                formatter,
                "No Git repositories were found under {}.",
                root.display()
            ),
            Self::SearchFailed { path, .. } => {
                write!(
                    formatter,
                    "Could not search {} for repositories.",
                    path.display()
                )
            }
        }
    }
}
