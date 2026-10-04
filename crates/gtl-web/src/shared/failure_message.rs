//! Formats typed failure reasons in the displayed language.
//!
//! Every reason has its own catalog message, so translations can agree in
//! gender and number instead of composing fragments. Verbatim Git and parser
//! output stays in the failure's diagnostic and is shown apart from this text.

use gtl_models::{
    diffs::CommitIdAbbreviation,
    failure::{
        DiffTextFailure, Failure, ProjectFailure, PushFailure, PushRefRejection, RemoteDiffFailure,
        RepositoryFailure, Resource, ScanFolderProblem, SettingsFailure, ViewerFailure,
    },
    settings::ViewerLanguage,
};

use super::i18n::t;

/// Explains `failure` in `language`.
pub(crate) fn failure_message(failure: &Failure, language: ViewerLanguage) -> String {
    match failure {
        Failure::Unexpected => t!(language, "failure-unexpected"),
        Failure::Unavailable => t!(language, "failure-unavailable"),
        Failure::Busy => t!(language, "failure-busy"),
        Failure::Changed => t!(language, "failure-changed"),
        Failure::Gone { resource } => gone_message(*resource, language),
        Failure::InvalidRequest { field } => {
            t!(language, "failure-invalid-request", field = field.as_str())
        }
        Failure::Push(failure) => push_message(failure, language),
        Failure::Settings(failure) => settings_message(failure, language),
        Failure::Viewer(failure) => viewer_message(failure, language),
        Failure::Project(failure) => project_message(failure, language),
        Failure::Repository(failure) => repository_message(failure, language),
        Failure::DiffText(failure) => diff_text_message(failure, language),
        Failure::RemoteDiff(failure) => remote_diff_message(failure, language),
        Failure::Unrecognized { class } => {
            t!(language, "failure-unrecognized", class = class.to_string())
        }
    }
}

fn gone_message(resource: Resource, language: ViewerLanguage) -> String {
    match resource {
        Resource::PushOperation => t!(language, "failure-gone-push-operation"),
        Resource::ViewerTab => t!(language, "failure-gone-viewer-tab"),
        Resource::Commit => t!(language, "failure-gone-commit"),
        Resource::DiffFile => t!(language, "failure-gone-diff-file"),
        Resource::SourceRange => t!(language, "failure-gone-source-range"),
        Resource::Snapshot => t!(language, "failure-gone-snapshot"),
        Resource::Project => t!(language, "failure-gone-project"),
    }
}

fn push_message(failure: &PushFailure, language: ViewerLanguage) -> String {
    match failure {
        PushFailure::NothingToPush => t!(language, "failure-push-nothing-to-push"),
        PushFailure::NoUpstream { branch } => {
            t!(
                language,
                "failure-push-no-upstream",
                branch = branch.to_string()
            )
        }
        PushFailure::Detached => t!(language, "failure-push-detached"),
        PushFailure::CheckoutChanged { current } => t!(
            language,
            "failure-push-checkout-changed",
            current = current.to_string()
        ),
        PushFailure::CommitRemoved { commit } => t!(
            language,
            "failure-push-commit-removed",
            commit = commit.abbreviated(CommitIdAbbreviation::TenCharacters)
        ),
        PushFailure::DestinationChanged => t!(language, "failure-push-destination-changed"),
        PushFailure::MultipleDestinations { remote } => t!(
            language,
            "failure-push-multiple-destinations",
            remote = remote.to_string()
        ),
        PushFailure::ReviewExpired => t!(language, "failure-push-review-expired"),
        PushFailure::HistoryFull { operations_max } => t!(
            language,
            "failure-push-history-full",
            operations = *operations_max
        ),
        PushFailure::Rejected { refs, .. } => {
            push_rejection_message(refs.first().map(|rejected| &rejected.reason), language)
        }
        PushFailure::GitFailed { .. } => t!(language, "failure-push-git-failed"),
    }
}

fn push_rejection_message(
    rejection: Option<&PushRefRejection>,
    language: ViewerLanguage,
) -> String {
    match rejection {
        Some(PushRefRejection::FetchFirst | PushRefRejection::NonFastForward) => {
            t!(language, "failure-push-remote-ahead")
        }
        Some(PushRefRejection::RemoteRejected { message }) => t!(
            language,
            "failure-push-remote-rejected-message",
            message = message.as_str()
        ),
        Some(PushRefRejection::Other { .. }) | None => t!(language, "failure-push-remote-rejected"),
    }
}

fn settings_message(failure: &SettingsFailure, language: ViewerLanguage) -> String {
    match failure {
        SettingsFailure::Invalid { path, .. } => t!(
            language,
            "failure-settings-invalid",
            path = path.display().to_string()
        ),
        SettingsFailure::Stale => t!(language, "failure-settings-stale"),
        SettingsFailure::Locked { wait_seconds } => {
            t!(language, "failure-settings-locked", seconds = *wait_seconds)
        }
        SettingsFailure::PathUnavailable => t!(language, "failure-settings-path-unavailable"),
    }
}

fn viewer_message(failure: &ViewerFailure, language: ViewerLanguage) -> String {
    match failure {
        ViewerFailure::SourcePreparing => t!(language, "failure-viewer-source-preparing"),
        ViewerFailure::RevealTooLarge => t!(language, "failure-viewer-reveal-too-large"),
        ViewerFailure::SnapshotNameInvalid { characters_max } => t!(
            language,
            "failure-viewer-snapshot-name-invalid",
            characters = *characters_max
        ),
        ViewerFailure::SnapshotPending => t!(language, "failure-viewer-snapshot-pending"),
        ViewerFailure::ModifiedFilesUnavailable => {
            t!(language, "failure-viewer-modified-files-unavailable")
        }
        ViewerFailure::RangeTooLarge => t!(language, "failure-viewer-range-too-large"),
        ViewerFailure::SearchTooLarge => t!(language, "failure-viewer-search-too-large"),
        ViewerFailure::ResponseTooLarge => t!(language, "failure-viewer-response-too-large"),
        ViewerFailure::FileNotInDiff => t!(language, "failure-viewer-file-not-in-diff"),
        ViewerFailure::FileDeleted => t!(language, "failure-viewer-file-deleted"),
        ViewerFailure::FileUnavailable => t!(language, "failure-viewer-file-unavailable"),
        ViewerFailure::FileOutsideRepository => {
            t!(language, "failure-viewer-file-outside-repository")
        }
        ViewerFailure::EditorFailed { .. } => t!(language, "failure-viewer-editor-failed"),
        ViewerFailure::SourceDirectoryMissing { path } => t!(
            language,
            "failure-viewer-source-directory-missing",
            path = path.display().to_string()
        ),
        ViewerFailure::SourceNotRepository { path } => t!(
            language,
            "failure-viewer-source-not-repository",
            path = path.display().to_string()
        ),
        ViewerFailure::SourceUnavailable => t!(language, "failure-viewer-source-unavailable"),
        ViewerFailure::RenderFailed => t!(language, "failure-viewer-render-failed"),
        ViewerFailure::CommitFailed => t!(language, "failure-viewer-commit-failed"),
        ViewerFailure::RowTooLarge => t!(language, "failure-viewer-row-too-large"),
    }
}

fn project_message(failure: &ProjectFailure, language: ViewerLanguage) -> String {
    match failure {
        ProjectFailure::CatalogueUnavailable => {
            t!(language, "failure-project-catalogue-unavailable")
        }
        ProjectFailure::AlreadyExists => t!(language, "failure-project-already-exists"),
        ProjectFailure::CatalogueFull { projects_max } => t!(
            language,
            "failure-project-catalogue-full",
            projects = *projects_max
        ),
        ProjectFailure::ScanFailed { .. } => t!(language, "failure-project-scan-failed"),
        ProjectFailure::ScanFolderInvalid { problem } => match problem {
            ScanFolderProblem::NotAbsolute => {
                t!(language, "failure-project-scan-folder-not-absolute")
            }
            ScanFolderProblem::NotDirectory => {
                t!(language, "failure-project-scan-folder-not-directory")
            }
            ScanFolderProblem::NotUtf8 => t!(language, "failure-project-scan-folder-not-utf8"),
        },
        ProjectFailure::HomeUnavailable => t!(language, "failure-project-home-unavailable"),
        ProjectFailure::TooManyRepositories { repositories_max } => t!(
            language,
            "failure-project-too-many-repositories",
            repositories = *repositories_max
        ),
        ProjectFailure::ComparisonBranchMissing { path, branch, .. } => t!(
            language,
            "failure-project-comparison-branch-missing",
            branch = branch.to_string(),
            path = path.to_string()
        ),
        ProjectFailure::RepositoryUnborn { path } => t!(
            language,
            "failure-project-repository-unborn",
            path = path.to_string()
        ),
        ProjectFailure::NoCommonAncestor { path, branch, .. } => t!(
            language,
            "failure-project-no-common-ancestor",
            branch = branch.to_string(),
            path = path.to_string()
        ),
        ProjectFailure::ScanStale => t!(language, "failure-project-scan-stale"),
        ProjectFailure::AlreadyManaged => t!(language, "failure-project-already-managed"),
        ProjectFailure::CommitCountUnavailable => {
            t!(language, "failure-project-commit-count-unavailable")
        }
    }
}

fn repository_message(failure: &RepositoryFailure, language: ViewerLanguage) -> String {
    match failure {
        RepositoryFailure::NotARepository { path } => t!(
            language,
            "failure-repository-not-a-repository",
            path = path.display().to_string()
        ),
        RepositoryFailure::NoRepositories { root } => t!(
            language,
            "failure-repository-no-repositories",
            root = root.display().to_string()
        ),
        RepositoryFailure::SearchFailed { path, .. } => t!(
            language,
            "failure-repository-search-failed",
            path = path.display().to_string()
        ),
    }
}

fn remote_diff_message(failure: &RemoteDiffFailure, language: ViewerLanguage) -> String {
    match failure {
        RemoteDiffFailure::UnsupportedOrigin => {
            t!(language, "failure-remote-diff-unsupported-origin")
        }
        RemoteDiffFailure::InvalidRange => t!(language, "failure-remote-diff-invalid-range"),
        RemoteDiffFailure::Unauthenticated => {
            t!(language, "failure-remote-diff-unauthenticated")
        }
        RemoteDiffFailure::NotFound => t!(language, "failure-remote-diff-not-found"),
        RemoteDiffFailure::RateLimited => t!(language, "failure-remote-diff-rate-limited"),
        RemoteDiffFailure::Rejected { status, .. } => t!(
            language,
            "failure-remote-diff-rejected",
            status = status.to_string()
        ),
    }
}

fn diff_text_message(failure: &DiffTextFailure, language: ViewerLanguage) -> String {
    match failure {
        DiffTextFailure::TooLarge { bytes_max } => t!(
            language,
            "failure-diff-text-too-large",
            mebibytes_max = (bytes_max / (1024 * 1024)).to_string()
        ),
        DiffTextFailure::NoFiles => t!(language, "failure-diff-text-no-files"),
        DiffTextFailure::InvalidFileHeader { .. } => {
            t!(language, "failure-diff-text-invalid-file-header")
        }
        DiffTextFailure::DuplicatePath { path } => t!(
            language,
            "failure-diff-text-duplicate-path",
            path = path.as_str()
        ),
        DiffTextFailure::Missing => t!(language, "failure-diff-text-missing"),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{Failure, PushFailure, ViewerFailure};

    use super::*;

    #[test]
    fn english_messages_keep_the_failure_display_text() {
        for failure in [
            Failure::Unexpected,
            Failure::Gone {
                resource: Resource::Snapshot,
            },
            Failure::InvalidRequest {
                field: "theme".to_owned(),
            },
            Failure::Push(PushFailure::HistoryFull { operations_max: 32 }),
            Failure::Viewer(ViewerFailure::SnapshotNameInvalid {
                characters_max: 200,
            }),
            Failure::Project(ProjectFailure::ScanFolderInvalid {
                problem: ScanFolderProblem::NotAbsolute,
            }),
        ] {
            assert_eq!(
                failure_message(&failure, ViewerLanguage::EnUs),
                failure.to_string()
            );
        }
    }
}
