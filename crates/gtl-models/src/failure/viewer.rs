use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{ErrorClass, ExternalDiagnostic, Failure, PublicFailure};

/// Why the desktop viewer cannot serve a view or tab operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerFailure {
    /// The diff source is still being prepared, so its contents are not readable yet.
    SourcePreparing,
    /// Revealing excluded files would exceed the viewer cache limit.
    RevealTooLarge,
    /// A snapshot name is empty, too long, or spans several lines.
    SnapshotNameInvalid { characters_max: u32 },
    /// The snapshot is still being saved.
    SnapshotPending,
    /// The tab cannot show modified files now, for example during a commit selection.
    ModifiedFilesUnavailable,
    /// The requested source text exceeds the response limit.
    RangeTooLarge,
    /// A search matched more than the viewer can return; narrow the query.
    SearchTooLarge,
    /// The view exceeds the transport limits for one response.
    ResponseTooLarge,
    /// The file is not part of the current diff.
    FileNotInDiff,
    /// Deleted files have no working-tree copy to open.
    FileDeleted,
    /// The working-tree file is missing or unreadable.
    FileUnavailable,
    /// The diff file resolves outside its repository.
    FileOutsideRepository,
    /// The configured editor could not be found or started.
    EditorFailed { diagnostic: ExternalDiagnostic },
    /// A tab's repository directory no longer exists; a live tab updates again when it returns.
    SourceDirectoryMissing { path: PathBuf },
    /// A tab's directory is no longer a Git repository; a live tab updates again when it is.
    SourceNotRepository { path: PathBuf },
    /// A tab's repository cannot be read; a live tab updates again when it is restored.
    SourceUnavailable,
    /// The diff could not be computed or rendered.
    RenderFailed,
    /// The selected commit's patch could not be rendered.
    CommitFailed,
    /// One diff row exceeds the viewer's row size limit.
    RowTooLarge,
}

impl ViewerFailure {
    /// Verbatim editor output that explains the failure.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::EditorFailed { diagnostic } => Some(diagnostic),
            _ => None,
        }
    }

    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::SnapshotNameInvalid { .. } => ErrorClass::InvalidArgument,
            Self::RevealTooLarge
            | Self::RowTooLarge
            | Self::RangeTooLarge
            | Self::SearchTooLarge
            | Self::ResponseTooLarge => ErrorClass::ResourceExhausted,
            Self::FileOutsideRepository => ErrorClass::PermissionDenied,
            Self::SourceUnavailable => ErrorClass::Unavailable,
            Self::RenderFailed | Self::CommitFailed => ErrorClass::Internal,
            Self::SourcePreparing
            | Self::SourceDirectoryMissing { .. }
            | Self::SourceNotRepository { .. }
            | Self::SnapshotPending
            | Self::ModifiedFilesUnavailable
            | Self::FileNotInDiff
            | Self::FileDeleted
            | Self::FileUnavailable
            | Self::EditorFailed { .. } => ErrorClass::FailedPrecondition,
        }
    }
}

impl PublicFailure for ViewerFailure {
    fn failure(&self) -> Failure {
        Failure::Viewer(self.clone())
    }
}

impl From<ViewerFailure> for Failure {
    fn from(failure: ViewerFailure) -> Self {
        Self::Viewer(failure)
    }
}

impl std::error::Error for ViewerFailure {}

impl fmt::Display for ViewerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourcePreparing => {
                formatter.write_str("The diff source is still being prepared. Try again shortly.")
            }
            Self::RevealTooLarge => {
                formatter.write_str("The revealed files exceed the viewer cache limit.")
            }
            Self::SnapshotNameInvalid { characters_max } => write!(
                formatter,
                "Snapshot names need 1 to {characters_max} characters on a single line."
            ),
            Self::SnapshotPending => {
                formatter.write_str("Wait for the snapshot to finish saving, then try again.")
            }
            Self::ModifiedFilesUnavailable => formatter
                .write_str("Modified files are unavailable while a commit selection is pending."),
            Self::RangeTooLarge => {
                formatter.write_str("This section contains too much text to display.")
            }
            Self::SearchTooLarge => {
                formatter.write_str("The search matched too much. Narrow the query.")
            }
            Self::ResponseTooLarge => {
                formatter.write_str("This view is too large to send to the viewer.")
            }
            Self::FileNotInDiff => formatter.write_str("The file is not in the current diff."),
            Self::FileDeleted => formatter.write_str("Deleted files cannot be opened."),
            Self::FileUnavailable => {
                formatter.write_str("The file is missing or unreadable in the working tree.")
            }
            Self::FileOutsideRepository => {
                formatter.write_str("The file resolves outside its repository.")
            }
            Self::EditorFailed { .. } => {
                formatter.write_str("The configured editor could not open the file.")
            }
            Self::SourceDirectoryMissing { path } => write!(
                formatter,
                "The repository directory {} was not found. Live tabs update again when it is restored.",
                path.display()
            ),
            Self::SourceNotRepository { path } => write!(
                formatter,
                "{} is not a Git repository. Live tabs update again when the repository is restored.",
                path.display()
            ),
            Self::SourceUnavailable => formatter.write_str(
                "The repository is unavailable. Live tabs update again when it is restored.",
            ),
            Self::RenderFailed => {
                formatter.write_str("The diff could not be rendered. Please retry.")
            }
            Self::CommitFailed => formatter.write_str(
                "The selected commit could not be rendered. Show all changes and retry.",
            ),
            Self::RowTooLarge => formatter.write_str("A diff row is too large to display."),
        }
    }
}
