use std::fmt;

use serde::{Deserialize, Serialize};

use super::{ErrorClass, ExternalDiagnostic, Failure, PublicFailure};
use crate::{paths::RepositoryRoot, projects::comparison::ComparisonBranch};

/// Why a project catalogue, import, or comparison operation cannot proceed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFailure {
    /// The project catalogue cannot be read now.
    CatalogueUnavailable,
    /// Another project already uses this ID, title, or source.
    AlreadyExists,
    /// The catalogue already holds its maximum number of projects.
    CatalogueFull { projects_max: u32 },
    /// The scan folder could not be opened or traversed.
    ScanFailed { diagnostic: ExternalDiagnostic },
    /// The scan folder input cannot name a folder to scan.
    ScanFolderInvalid { problem: ScanFolderProblem },
    /// The home folder that `~` refers to is unavailable.
    HomeUnavailable,
    /// The folder contains more repositories than one import accepts.
    TooManyRepositories { repositories_max: u32 },
    /// The project's local comparison branch does not exist in this repository.
    ComparisonBranchMissing {
        path: RepositoryRoot,
        branch: ComparisonBranch,
        /// The catalogued project whose setting names `branch`; `None` when the default applies.
        project: Option<RepositoryRoot>,
    },
    /// The repository has no commits to compare.
    RepositoryUnborn { path: RepositoryRoot },
    /// The comparison branch and `HEAD` share no history.
    NoCommonAncestor {
        path: RepositoryRoot,
        branch: ComparisonBranch,
        /// The catalogued project whose setting names `branch`; `None` when the default applies.
        project: Option<RepositoryRoot>,
    },
    /// The folder or catalogue changed after the scan that proposed this import.
    ScanStale,
    /// The repository is already a managed project.
    AlreadyManaged,
    /// Git could not count the commits ahead of the comparison branch.
    CommitCountUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanFolderProblem {
    NotAbsolute,
    NotDirectory,
    NotUtf8,
}

impl ProjectFailure {
    /// The project comparison setting that would resolve this failure, with its current branch.
    #[must_use]
    pub const fn comparison_setting(&self) -> Option<(&RepositoryRoot, &ComparisonBranch)> {
        match self {
            Self::ComparisonBranchMissing {
                branch,
                project: Some(project),
                ..
            }
            | Self::NoCommonAncestor {
                branch,
                project: Some(project),
                ..
            } => Some((project, branch)),
            _ => None,
        }
    }

    /// Verbatim filesystem output that explains the failure.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::ScanFailed { diagnostic } => Some(diagnostic),
            _ => None,
        }
    }

    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::AlreadyExists | Self::AlreadyManaged => ErrorClass::AlreadyExists,
            Self::CatalogueFull { .. } | Self::TooManyRepositories { .. } => {
                ErrorClass::ResourceExhausted
            }
            Self::ScanFolderInvalid { .. } => ErrorClass::InvalidArgument,
            Self::CommitCountUnavailable => ErrorClass::Unavailable,
            Self::CatalogueUnavailable
            | Self::ScanStale
            | Self::ScanFailed { .. }
            | Self::HomeUnavailable
            | Self::ComparisonBranchMissing { .. }
            | Self::RepositoryUnborn { .. }
            | Self::NoCommonAncestor { .. } => ErrorClass::FailedPrecondition,
        }
    }
}

impl PublicFailure for ProjectFailure {
    fn failure(&self) -> Failure {
        Failure::Project(self.clone())
    }
}

impl From<ProjectFailure> for Failure {
    fn from(failure: ProjectFailure) -> Self {
        Self::Project(failure)
    }
}

impl std::error::Error for ProjectFailure {}

impl fmt::Display for ProjectFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogueUnavailable => formatter.write_str(
                "The project catalogue is unavailable. Check the GTL server and try again.",
            ),
            Self::AlreadyExists => {
                formatter.write_str("A project with this ID, title, or source already exists.")
            }
            Self::CatalogueFull { projects_max } => write!(
                formatter,
                "The project catalogue already holds {projects_max} projects."
            ),
            Self::ScanFailed { .. } => formatter
                .write_str("Could not scan this folder. Check that it exists and is readable."),
            Self::ScanFolderInvalid { problem } => formatter.write_str(match problem {
                ScanFolderProblem::NotAbsolute => {
                    "Enter an absolute folder path or one that starts with ~/."
                }
                ScanFolderProblem::NotDirectory => "The scan path is not a folder.",
                ScanFolderProblem::NotUtf8 => "The scan folder path is not valid UTF-8.",
            }),
            Self::HomeUnavailable => formatter.write_str("The home folder is unavailable."),
            Self::TooManyRepositories { repositories_max } => write!(
                formatter,
                "This folder contains more than {repositories_max} repositories. Choose a narrower folder."
            ),
            Self::ComparisonBranchMissing { path, branch, .. } => write!(
                formatter,
                "Local comparison branch '{branch}' is missing in {path}."
            ),
            Self::RepositoryUnborn { path } => {
                write!(
                    formatter,
                    "The repository at {path} has no commits to compare."
                )
            }
            Self::NoCommonAncestor { path, branch, .. } => write!(
                formatter,
                "Local comparison branch '{branch}' and HEAD have no common ancestor in {path}."
            ),
            Self::ScanStale => {
                formatter.write_str("This folder changed since the scan. Scan again.")
            }
            Self::AlreadyManaged => {
                formatter.write_str("This repository is already a managed project.")
            }
            Self::CommitCountUnavailable => formatter
                .write_str("Git could not count the commits ahead of the comparison branch."),
        }
    }
}
