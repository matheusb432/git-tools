//! Presents typed failures: severity per reason, the localized message, and verbatim detail.
//!
//! Messages come from the catalog keyed by each reason, never from server text.

use dioxus::CapturedError;
use gtl_models::{
    failure::{
        DiffTextFailure, Failure, ProjectFailure, PushFailure, RemoteDiffFailure,
        RepositoryFailure, SettingsFailure, ViewerFailure,
    },
    settings::ViewerLanguage,
};

use super::{
    failure_message::failure_message,
    i18n::t,
    ui::{ToastHandle, ToastKind, ToastText},
    viewer_client::{ViewerClientError, captured_client_error},
};

/// How urgently the viewer reports `failure`.
pub(crate) const fn failure_severity(failure: &Failure) -> ToastKind {
    match failure {
        Failure::Unexpected | Failure::InvalidRequest { .. } | Failure::Unrecognized { .. } => {
            ToastKind::Error
        }
        Failure::Unavailable | Failure::Busy | Failure::Changed | Failure::Gone { .. } => {
            ToastKind::Warn
        }
        Failure::Push(failure) => push_severity(failure),
        Failure::Settings(failure) => settings_severity(failure),
        Failure::Viewer(failure) => viewer_severity(failure),
        Failure::Project(failure) => project_severity(failure),
        Failure::Repository(RepositoryFailure::SearchFailed { .. })
        | Failure::DiffText(
            DiffTextFailure::TooLarge { .. }
            | DiffTextFailure::NoFiles
            | DiffTextFailure::InvalidFileHeader { .. }
            | DiffTextFailure::DuplicatePath { .. },
        ) => ToastKind::Error,
        Failure::Repository(
            RepositoryFailure::NotARepository { .. } | RepositoryFailure::NoRepositories { .. },
        )
        | Failure::DiffText(DiffTextFailure::Missing) => ToastKind::Warn,
        Failure::RemoteDiff(failure) => remote_diff_severity(failure),
    }
}

const fn remote_diff_severity(failure: &RemoteDiffFailure) -> ToastKind {
    match failure {
        RemoteDiffFailure::RateLimited => ToastKind::Warn,
        RemoteDiffFailure::UnsupportedOrigin
        | RemoteDiffFailure::InvalidRange
        | RemoteDiffFailure::Unauthenticated
        | RemoteDiffFailure::NotFound
        | RemoteDiffFailure::Rejected { .. } => ToastKind::Error,
    }
}

const fn push_severity(failure: &PushFailure) -> ToastKind {
    match failure {
        PushFailure::NothingToPush => ToastKind::Info,
        // Git or the remote refused the push itself.
        PushFailure::Rejected { .. } | PushFailure::GitFailed { .. } => ToastKind::Error,
        // Local state changed or needs configuration; reviewing again or fixing setup recovers.
        PushFailure::NoUpstream { .. }
        | PushFailure::Detached
        | PushFailure::CheckoutChanged { .. }
        | PushFailure::CommitRemoved { .. }
        | PushFailure::DestinationChanged
        | PushFailure::MultipleDestinations { .. }
        | PushFailure::ReviewExpired
        | PushFailure::HistoryFull { .. } => ToastKind::Warn,
    }
}

const fn settings_severity(failure: &SettingsFailure) -> ToastKind {
    match failure {
        SettingsFailure::Invalid { .. } | SettingsFailure::PathUnavailable => ToastKind::Error,
        // Reloading or retrying recovers from a concurrent edit.
        SettingsFailure::Stale | SettingsFailure::Locked { .. } => ToastKind::Warn,
    }
}

const fn viewer_severity(failure: &ViewerFailure) -> ToastKind {
    match failure {
        ViewerFailure::SourcePreparing => ToastKind::Info,
        // Waiting, narrowing the request, or changing the input recovers; live sources resume
        // updates by themselves.
        ViewerFailure::RevealTooLarge
        | ViewerFailure::SnapshotNameInvalid { .. }
        | ViewerFailure::SnapshotPending
        | ViewerFailure::ModifiedFilesUnavailable
        | ViewerFailure::RangeTooLarge
        | ViewerFailure::SearchTooLarge
        | ViewerFailure::FileNotInDiff
        | ViewerFailure::FileDeleted
        | ViewerFailure::FileUnavailable
        | ViewerFailure::SourceDirectoryMissing { .. }
        | ViewerFailure::SourceNotRepository { .. }
        | ViewerFailure::SourceUnavailable
        | ViewerFailure::RowTooLarge => ToastKind::Warn,
        ViewerFailure::ResponseTooLarge
        | ViewerFailure::FileOutsideRepository
        | ViewerFailure::EditorFailed { .. }
        | ViewerFailure::RenderFailed
        | ViewerFailure::CommitFailed => ToastKind::Error,
    }
}

const fn project_severity(failure: &ProjectFailure) -> ToastKind {
    match failure {
        ProjectFailure::ScanFailed { .. } | ProjectFailure::HomeUnavailable => ToastKind::Error,
        // The user can change the input, the folder, or the comparison branch.
        ProjectFailure::CatalogueUnavailable
        | ProjectFailure::AlreadyExists
        | ProjectFailure::CatalogueFull { .. }
        | ProjectFailure::ScanFolderInvalid { .. }
        | ProjectFailure::TooManyRepositories { .. }
        | ProjectFailure::ComparisonBranchMissing { .. }
        | ProjectFailure::RepositoryUnborn { .. }
        | ProjectFailure::NoCommonAncestor { .. }
        | ProjectFailure::ScanStale
        | ProjectFailure::AlreadyManaged
        | ProjectFailure::CommitCountUnavailable => ToastKind::Warn,
    }
}

/// How urgently the viewer reports a client error.
pub(crate) const fn client_error_severity(error: &ViewerClientError) -> ToastKind {
    match error {
        ViewerClientError::Failed(failure) => failure_severity(failure),
        // The connection recovers on its own or on refresh.
        ViewerClientError::Disconnected | ViewerClientError::StreamClosed => ToastKind::Warn,
        ViewerClientError::ProtocolMismatch
        | ViewerClientError::InvalidMessage
        | ViewerClientError::Desktop { .. } => ToastKind::Error,
    }
}

/// Explains a viewer client error in `language`.
pub(crate) fn client_error_message(error: &ViewerClientError, language: ViewerLanguage) -> String {
    match error {
        ViewerClientError::ProtocolMismatch => t!(language, "client-error-protocol-mismatch"),
        ViewerClientError::Disconnected => t!(language, "client-error-disconnected"),
        ViewerClientError::InvalidMessage => t!(language, "client-error-invalid-message"),
        ViewerClientError::StreamClosed => t!(language, "client-error-stream-closed"),
        ViewerClientError::Desktop { .. } => t!(language, "client-error-desktop"),
        ViewerClientError::Failed(failure) => failure_message(failure, language),
    }
}

/// Explains an action error that captured a viewer client error in `language`.
pub(crate) fn captured_error_message(error: &CapturedError, language: ViewerLanguage) -> String {
    captured_client_error(error).map_or_else(
        || error.to_string(),
        |error| client_error_message(error, language),
    )
}

/// Whether the settings file itself is invalid, which routes the viewer to settings recovery.
pub(crate) const fn is_invalid_settings(error: &ViewerClientError) -> bool {
    matches!(
        error,
        ViewerClientError::Failed(Failure::Settings(SettingsFailure::Invalid { .. }))
    )
}

impl ToastHandle {
    /// Shows `failure` with its severity, message, and any verbatim diagnostic.
    pub(crate) fn failure(self, failure: &Failure) {
        let message = failure.clone();
        self.show(
            failure_severity(failure),
            ToastText::localized(move |language| failure_message(&message, language)),
            failure
                .diagnostic()
                .filter(|diagnostic| !diagnostic.is_empty())
                .map(ToString::to_string),
        );
    }

    /// Shows a viewer client error with its severity, message, and any verbatim diagnostic.
    pub(crate) fn client_error(self, error: &ViewerClientError) {
        let message = error.clone();
        self.show(
            client_error_severity(error),
            ToastText::localized(move |language| client_error_message(&message, language)),
            error
                .diagnostic()
                .filter(|diagnostic| !diagnostic.is_empty())
                .map(ToString::to_string),
        );
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{ExternalDiagnostic, Failure, PushFailure, ViewerFailure};

    use super::failure_severity;
    use crate::shared::ui::ToastKind;

    #[test]
    fn remote_rejections_are_errors_while_recoverable_refusals_warn() {
        assert_eq!(
            failure_severity(&Failure::Push(PushFailure::Rejected {
                refs: Vec::new(),
                diagnostic: ExternalDiagnostic::new("! [rejected]"),
            })),
            ToastKind::Error
        );
        assert_eq!(
            failure_severity(&Failure::Push(PushFailure::ReviewExpired)),
            ToastKind::Warn
        );
        assert_eq!(
            failure_severity(&Failure::Push(PushFailure::NothingToPush)),
            ToastKind::Info
        );
        assert_eq!(
            failure_severity(&Failure::Viewer(ViewerFailure::SourcePreparing)),
            ToastKind::Info
        );
        assert_eq!(failure_severity(&Failure::Unexpected), ToastKind::Error);
    }
}
