//! Protobuf codec for typed failures and, with `grpc`, their `google.rpc.Status` encoding.

#[cfg(feature = "grpc")]
mod status;

use std::path::PathBuf;

use gtl_models::{
    failure::{
        ExternalDiagnostic, Failure, ProjectFailure, PushFailure, PushRefRejection,
        RejectedPushRef, RepositoryFailure, Resource, ScanFolderProblem, SettingsFailure,
        ViewerFailure,
    },
    paths::RepositoryRoot,
};
#[cfg(feature = "grpc")]
pub use status::{StatusFailure, class_code, code_class, decode_status, encode_status};

use crate::v1;

#[must_use]
pub fn encode_failure(failure: &Failure) -> v1::Failure {
    use v1::failure::Reason;
    let reason = match failure {
        Failure::Unexpected => Some(Reason::Unexpected(v1::FailureUnexpected {})),
        Failure::Unavailable => Some(Reason::Unavailable(v1::FailureUnavailable {})),
        Failure::Busy => Some(Reason::Busy(v1::FailureBusy {})),
        Failure::Changed => Some(Reason::Changed(v1::FailureChanged {})),
        Failure::Gone { resource } => Some(Reason::Gone(v1::FailureGone {
            resource: encode_resource(*resource).into(),
        })),
        Failure::InvalidRequest { field } => {
            Some(Reason::InvalidRequest(v1::FailureInvalidRequest {
                field: field.clone(),
            }))
        }
        Failure::Push(failure) => Some(Reason::Push(encode_push_failure(failure))),
        Failure::Settings(failure) => Some(Reason::Settings(encode_settings_failure(failure))),
        Failure::Viewer(failure) => Some(Reason::Viewer(encode_viewer_failure(failure))),
        Failure::Project(failure) => Some(Reason::Project(encode_project_failure(failure))),
        Failure::Repository(failure) => {
            Some(Reason::Repository(encode_repository_failure(failure)))
        }
        // A relayed unknown reason stays unknown; receivers fall back to the status code.
        Failure::Unrecognized { .. } => None,
    };
    v1::Failure { reason }
}

/// Returns `None` when the reason is absent, unknown to this build, or malformed.
#[must_use]
pub fn decode_failure(failure: v1::Failure) -> Option<Failure> {
    use v1::failure::Reason;
    Some(match failure.reason? {
        Reason::Unexpected(_) => Failure::Unexpected,
        Reason::Unavailable(_) => Failure::Unavailable,
        Reason::Busy(_) => Failure::Busy,
        Reason::Changed(_) => Failure::Changed,
        Reason::Gone(gone) => Failure::Gone {
            resource: decode_resource(gone.resource())?,
        },
        Reason::InvalidRequest(request) => Failure::InvalidRequest {
            field: request.field,
        },
        Reason::Push(failure) => Failure::Push(decode_push_failure(failure)?),
        Reason::Settings(failure) => Failure::Settings(decode_settings_failure(failure)?),
        Reason::Viewer(failure) => Failure::Viewer(decode_viewer_failure(failure)?),
        Reason::Project(failure) => Failure::Project(decode_project_failure(failure)?),
        Reason::Repository(failure) => Failure::Repository(decode_repository_failure(failure)?),
    })
}

const fn encode_resource(resource: Resource) -> v1::FailureResource {
    match resource {
        Resource::PushOperation => v1::FailureResource::PushOperation,
        Resource::ViewerTab => v1::FailureResource::ViewerTab,
        Resource::Commit => v1::FailureResource::Commit,
        Resource::DiffFile => v1::FailureResource::DiffFile,
        Resource::SourceRange => v1::FailureResource::SourceRange,
        Resource::Snapshot => v1::FailureResource::Snapshot,
        Resource::Project => v1::FailureResource::Project,
    }
}

const fn decode_resource(resource: v1::FailureResource) -> Option<Resource> {
    match resource {
        v1::FailureResource::PushOperation => Some(Resource::PushOperation),
        v1::FailureResource::ViewerTab => Some(Resource::ViewerTab),
        v1::FailureResource::Commit => Some(Resource::Commit),
        v1::FailureResource::DiffFile => Some(Resource::DiffFile),
        v1::FailureResource::SourceRange => Some(Resource::SourceRange),
        v1::FailureResource::Snapshot => Some(Resource::Snapshot),
        v1::FailureResource::Project => Some(Resource::Project),
        v1::FailureResource::Unspecified => None,
    }
}

#[must_use]
pub fn encode_push_failure(failure: &PushFailure) -> v1::PushFailure {
    use v1::push_failure::Reason;
    let reason = match failure {
        PushFailure::NothingToPush => Reason::NothingToPush(v1::PushFailureNothingToPush {}),
        PushFailure::NoUpstream { branch } => Reason::NoUpstream(v1::PushFailureNoUpstream {
            branch: branch.to_string(),
        }),
        PushFailure::Detached => Reason::Detached(v1::PushFailureDetached {}),
        PushFailure::CheckoutChanged { current } => {
            Reason::CheckoutChanged(v1::PushFailureCheckoutChanged {
                current: current.to_string(),
            })
        }
        PushFailure::CommitRemoved { commit } => {
            Reason::CommitRemoved(v1::PushFailureCommitRemoved {
                commit: commit.to_string(),
            })
        }
        PushFailure::DestinationChanged => {
            Reason::DestinationChanged(v1::PushFailureDestinationChanged {})
        }
        PushFailure::MultipleDestinations { remote } => {
            Reason::MultipleDestinations(v1::PushFailureMultipleDestinations {
                remote: remote.to_string(),
            })
        }
        PushFailure::ReviewExpired => Reason::ReviewExpired(v1::PushFailureReviewExpired {}),
        PushFailure::HistoryFull { operations_max } => {
            Reason::HistoryFull(v1::PushFailureHistoryFull {
                operations_max: *operations_max,
            })
        }
        PushFailure::Rejected { refs, diagnostic } => Reason::Rejected(v1::PushFailureRejected {
            refs: refs.iter().map(encode_rejected_ref).collect(),
            diagnostic: diagnostic.to_string(),
        }),
        PushFailure::GitFailed { diagnostic } => Reason::GitFailed(v1::PushFailureGitFailed {
            diagnostic: diagnostic.to_string(),
        }),
    };
    v1::PushFailure {
        reason: Some(reason),
    }
}

/// Returns `None` when the reason is absent, unknown to this build, or malformed.
#[must_use]
pub fn decode_push_failure(failure: v1::PushFailure) -> Option<PushFailure> {
    use v1::push_failure::Reason;
    Some(match failure.reason? {
        Reason::NothingToPush(_) => PushFailure::NothingToPush,
        Reason::NoUpstream(value) => PushFailure::NoUpstream {
            branch: value.branch.try_into().ok()?,
        },
        Reason::Detached(_) => PushFailure::Detached,
        Reason::CheckoutChanged(value) => PushFailure::CheckoutChanged {
            current: value.current.try_into().ok()?,
        },
        Reason::CommitRemoved(value) => PushFailure::CommitRemoved {
            commit: value.commit.parse().ok()?,
        },
        Reason::DestinationChanged(_) => PushFailure::DestinationChanged,
        Reason::MultipleDestinations(value) => PushFailure::MultipleDestinations {
            remote: value.remote.try_into().ok()?,
        },
        Reason::ReviewExpired(_) => PushFailure::ReviewExpired,
        Reason::HistoryFull(value) => PushFailure::HistoryFull {
            operations_max: value.operations_max,
        },
        Reason::Rejected(value) => PushFailure::Rejected {
            refs: value
                .refs
                .into_iter()
                .map(decode_rejected_ref)
                .collect::<Option<_>>()?,
            diagnostic: ExternalDiagnostic::from(value.diagnostic),
        },
        Reason::GitFailed(value) => PushFailure::GitFailed {
            diagnostic: ExternalDiagnostic::from(value.diagnostic),
        },
    })
}

fn encode_rejected_ref(rejected: &RejectedPushRef) -> v1::PushRejectedRef {
    let (reason, detail) = match &rejected.reason {
        PushRefRejection::FetchFirst => (v1::PushRefRejection::FetchFirst, String::new()),
        PushRefRejection::NonFastForward => (v1::PushRefRejection::NonFastForward, String::new()),
        PushRefRejection::RemoteRejected { message } => {
            (v1::PushRefRejection::RemoteRejected, message.to_string())
        }
        PushRefRejection::Other { summary } => (v1::PushRefRejection::Other, summary.to_string()),
    };
    v1::PushRejectedRef {
        destination: rejected.destination.to_string(),
        reason: reason.into(),
        detail,
    }
}

fn decode_rejected_ref(rejected: v1::PushRejectedRef) -> Option<RejectedPushRef> {
    let reason = match rejected.reason() {
        v1::PushRefRejection::FetchFirst => PushRefRejection::FetchFirst,
        v1::PushRefRejection::NonFastForward => PushRefRejection::NonFastForward,
        v1::PushRefRejection::RemoteRejected => PushRefRejection::RemoteRejected {
            message: ExternalDiagnostic::from(rejected.detail),
        },
        // An unknown rejection from a newer peer keeps its summary.
        v1::PushRefRejection::Other | v1::PushRefRejection::Unspecified => {
            PushRefRejection::Other {
                summary: ExternalDiagnostic::from(rejected.detail),
            }
        }
    };
    Some(RejectedPushRef {
        destination: rejected.destination.try_into().ok()?,
        reason,
    })
}

#[must_use]
pub fn encode_settings_failure(failure: &SettingsFailure) -> v1::SettingsFailure {
    use v1::settings_failure::Reason;
    let reason = match failure {
        SettingsFailure::Invalid { path, diagnostic } => {
            Reason::Invalid(v1::SettingsFailureInvalid {
                path: path.to_string_lossy().into_owned(),
                diagnostic: diagnostic.to_string(),
            })
        }
        SettingsFailure::Stale => Reason::Stale(v1::SettingsFailureStale {}),
        SettingsFailure::Locked { wait_seconds } => Reason::Locked(v1::SettingsFailureLocked {
            wait_seconds: *wait_seconds,
        }),
        SettingsFailure::PathUnavailable => {
            Reason::PathUnavailable(v1::SettingsFailurePathUnavailable {})
        }
    };
    v1::SettingsFailure {
        reason: Some(reason),
    }
}

/// Returns `None` when the reason is absent, unknown to this build, or malformed.
#[must_use]
pub fn decode_settings_failure(failure: v1::SettingsFailure) -> Option<SettingsFailure> {
    use v1::settings_failure::Reason;
    Some(match failure.reason? {
        Reason::Invalid(value) => SettingsFailure::Invalid {
            path: PathBuf::from(value.path),
            diagnostic: ExternalDiagnostic::from(value.diagnostic),
        },
        Reason::Stale(_) => SettingsFailure::Stale,
        Reason::Locked(value) => SettingsFailure::Locked {
            wait_seconds: value.wait_seconds,
        },
        Reason::PathUnavailable(_) => SettingsFailure::PathUnavailable,
    })
}

#[must_use]
pub fn encode_viewer_failure(failure: &ViewerFailure) -> v1::ViewerFailure {
    use v1::viewer_failure::Reason;
    let reason = match failure {
        ViewerFailure::SourcePreparing => {
            Reason::SourcePreparing(v1::ViewerFailureSourcePreparing {})
        }
        ViewerFailure::RevealTooLarge => Reason::RevealTooLarge(v1::ViewerFailureRevealTooLarge {}),
        ViewerFailure::SnapshotNameInvalid { characters_max } => {
            Reason::SnapshotNameInvalid(v1::ViewerFailureSnapshotNameInvalid {
                characters_max: *characters_max,
            })
        }
        ViewerFailure::SnapshotPending => {
            Reason::SnapshotPending(v1::ViewerFailureSnapshotPending {})
        }
        ViewerFailure::ModifiedFilesUnavailable => {
            Reason::ModifiedFilesUnavailable(v1::ViewerFailureModifiedFilesUnavailable {})
        }
        ViewerFailure::RangeTooLarge => Reason::RangeTooLarge(v1::ViewerFailureRangeTooLarge {}),
        ViewerFailure::SearchTooLarge => Reason::SearchTooLarge(v1::ViewerFailureSearchTooLarge {}),
        ViewerFailure::ResponseTooLarge => {
            Reason::ResponseTooLarge(v1::ViewerFailureResponseTooLarge {})
        }
        ViewerFailure::FileNotInDiff => Reason::FileNotInDiff(v1::ViewerFailureFileNotInDiff {}),
        ViewerFailure::FileDeleted => Reason::FileDeleted(v1::ViewerFailureFileDeleted {}),
        ViewerFailure::FileUnavailable => {
            Reason::FileUnavailable(v1::ViewerFailureFileUnavailable {})
        }
        ViewerFailure::FileOutsideRepository => {
            Reason::FileOutsideRepository(v1::ViewerFailureFileOutsideRepository {})
        }
        ViewerFailure::EditorFailed { diagnostic } => {
            Reason::EditorFailed(v1::ViewerFailureEditorFailed {
                diagnostic: diagnostic.to_string(),
            })
        }
        ViewerFailure::SourceDirectoryMissing { path } => {
            Reason::SourceDirectoryMissing(v1::ViewerFailureSourceDirectoryMissing {
                path: path.to_string_lossy().into_owned(),
            })
        }
        ViewerFailure::SourceNotRepository { path } => {
            Reason::SourceNotRepository(v1::ViewerFailureSourceNotRepository {
                path: path.to_string_lossy().into_owned(),
            })
        }
        ViewerFailure::SourceUnavailable => {
            Reason::SourceUnavailable(v1::ViewerFailureSourceUnavailable {})
        }
        ViewerFailure::RenderFailed => Reason::RenderFailed(v1::ViewerFailureRenderFailed {}),
        ViewerFailure::CommitFailed => Reason::CommitFailed(v1::ViewerFailureCommitFailed {}),
        ViewerFailure::RowTooLarge => Reason::RowTooLarge(v1::ViewerFailureRowTooLarge {}),
    };
    v1::ViewerFailure {
        reason: Some(reason),
    }
}

/// Returns `None` when the reason is absent or unknown to this build.
#[must_use]
pub fn decode_viewer_failure(failure: v1::ViewerFailure) -> Option<ViewerFailure> {
    use v1::viewer_failure::Reason;
    Some(match failure.reason? {
        Reason::SourcePreparing(_) => ViewerFailure::SourcePreparing,
        Reason::RevealTooLarge(_) => ViewerFailure::RevealTooLarge,
        Reason::SnapshotNameInvalid(value) => ViewerFailure::SnapshotNameInvalid {
            characters_max: value.characters_max,
        },
        Reason::SnapshotPending(_) => ViewerFailure::SnapshotPending,
        Reason::ModifiedFilesUnavailable(_) => ViewerFailure::ModifiedFilesUnavailable,
        Reason::RangeTooLarge(_) => ViewerFailure::RangeTooLarge,
        Reason::SearchTooLarge(_) => ViewerFailure::SearchTooLarge,
        Reason::ResponseTooLarge(_) => ViewerFailure::ResponseTooLarge,
        Reason::FileNotInDiff(_) => ViewerFailure::FileNotInDiff,
        Reason::FileDeleted(_) => ViewerFailure::FileDeleted,
        Reason::FileUnavailable(_) => ViewerFailure::FileUnavailable,
        Reason::FileOutsideRepository(_) => ViewerFailure::FileOutsideRepository,
        Reason::EditorFailed(value) => ViewerFailure::EditorFailed {
            diagnostic: ExternalDiagnostic::from(value.diagnostic),
        },
        Reason::SourceDirectoryMissing(value) => ViewerFailure::SourceDirectoryMissing {
            path: PathBuf::from(value.path),
        },
        Reason::SourceNotRepository(value) => ViewerFailure::SourceNotRepository {
            path: PathBuf::from(value.path),
        },
        Reason::SourceUnavailable(_) => ViewerFailure::SourceUnavailable,
        Reason::RenderFailed(_) => ViewerFailure::RenderFailed,
        Reason::CommitFailed(_) => ViewerFailure::CommitFailed,
        Reason::RowTooLarge(_) => ViewerFailure::RowTooLarge,
    })
}

#[must_use]
pub fn encode_project_failure(failure: &ProjectFailure) -> v1::ProjectFailure {
    use v1::project_failure::Reason;
    let reason = match failure {
        ProjectFailure::CatalogueUnavailable => {
            Reason::CatalogueUnavailable(v1::ProjectFailureCatalogueUnavailable {})
        }
        ProjectFailure::AlreadyExists => Reason::AlreadyExists(v1::ProjectFailureAlreadyExists {}),
        ProjectFailure::CatalogueFull { projects_max } => {
            Reason::CatalogueFull(v1::ProjectFailureCatalogueFull {
                projects_max: *projects_max,
            })
        }
        ProjectFailure::ScanFailed { diagnostic } => {
            Reason::ScanFailed(v1::ProjectFailureScanFailed {
                diagnostic: diagnostic.to_string(),
            })
        }
        ProjectFailure::ScanFolderInvalid { problem } => {
            Reason::ScanFolderInvalid(v1::ProjectFailureScanFolderInvalid {
                problem: encode_scan_folder_problem(*problem).into(),
            })
        }
        ProjectFailure::HomeUnavailable => {
            Reason::HomeUnavailable(v1::ProjectFailureHomeUnavailable {})
        }
        ProjectFailure::TooManyRepositories { repositories_max } => {
            Reason::TooManyRepositories(v1::ProjectFailureTooManyRepositories {
                repositories_max: *repositories_max,
            })
        }
        ProjectFailure::ComparisonBranchMissing {
            path,
            branch,
            project,
        } => Reason::ComparisonBranchMissing(v1::ProjectFailureComparisonBranchMissing {
            path: path.to_string(),
            branch: branch.to_string(),
            project: project.as_ref().map(ToString::to_string),
        }),
        ProjectFailure::RepositoryUnborn { path } => {
            Reason::RepositoryUnborn(v1::ProjectFailureRepositoryUnborn {
                path: path.to_string(),
            })
        }
        ProjectFailure::NoCommonAncestor {
            path,
            branch,
            project,
        } => Reason::NoCommonAncestor(v1::ProjectFailureNoCommonAncestor {
            path: path.to_string(),
            branch: branch.to_string(),
            project: project.as_ref().map(ToString::to_string),
        }),
        ProjectFailure::ScanStale => Reason::ScanStale(v1::ProjectFailureScanStale {}),
        ProjectFailure::CommitCountUnavailable => {
            Reason::CommitCountUnavailable(v1::ProjectFailureCommitCountUnavailable {})
        }
        ProjectFailure::AlreadyManaged => {
            Reason::AlreadyManaged(v1::ProjectFailureAlreadyManaged {})
        }
    };
    v1::ProjectFailure {
        reason: Some(reason),
    }
}

/// Returns `None` when the reason is absent, unknown to this build, or malformed.
#[must_use]
pub fn decode_project_failure(failure: v1::ProjectFailure) -> Option<ProjectFailure> {
    use v1::project_failure::Reason;
    let path = |path: String| RepositoryRoot::try_new(PathBuf::from(path)).ok();
    // An absent path decodes to `None`; a malformed one rejects the failure.
    let optional_path =
        |value: Option<String>| value.map_or(Some(None), |value| path(value).map(Some));
    Some(match failure.reason? {
        Reason::CatalogueUnavailable(_) => ProjectFailure::CatalogueUnavailable,
        Reason::AlreadyExists(_) => ProjectFailure::AlreadyExists,
        Reason::CatalogueFull(value) => ProjectFailure::CatalogueFull {
            projects_max: value.projects_max,
        },
        Reason::ScanFailed(value) => ProjectFailure::ScanFailed {
            diagnostic: ExternalDiagnostic::from(value.diagnostic),
        },
        Reason::ScanFolderInvalid(value) => ProjectFailure::ScanFolderInvalid {
            problem: decode_scan_folder_problem(value.problem())?,
        },
        Reason::HomeUnavailable(_) => ProjectFailure::HomeUnavailable,
        Reason::TooManyRepositories(value) => ProjectFailure::TooManyRepositories {
            repositories_max: value.repositories_max,
        },
        Reason::ComparisonBranchMissing(value) => ProjectFailure::ComparisonBranchMissing {
            path: path(value.path)?,
            branch: value.branch.try_into().ok()?,
            project: optional_path(value.project)?,
        },
        Reason::RepositoryUnborn(value) => ProjectFailure::RepositoryUnborn {
            path: path(value.path)?,
        },
        Reason::NoCommonAncestor(value) => ProjectFailure::NoCommonAncestor {
            path: path(value.path)?,
            branch: value.branch.try_into().ok()?,
            project: optional_path(value.project)?,
        },
        Reason::ScanStale(_) => ProjectFailure::ScanStale,
        Reason::AlreadyManaged(_) => ProjectFailure::AlreadyManaged,
        Reason::CommitCountUnavailable(_) => ProjectFailure::CommitCountUnavailable,
    })
}

const fn encode_scan_folder_problem(problem: ScanFolderProblem) -> v1::ScanFolderProblem {
    match problem {
        ScanFolderProblem::NotAbsolute => v1::ScanFolderProblem::NotAbsolute,
        ScanFolderProblem::NotDirectory => v1::ScanFolderProblem::NotDirectory,
        ScanFolderProblem::NotUtf8 => v1::ScanFolderProblem::NotUtf8,
    }
}

const fn decode_scan_folder_problem(problem: v1::ScanFolderProblem) -> Option<ScanFolderProblem> {
    match problem {
        v1::ScanFolderProblem::NotAbsolute => Some(ScanFolderProblem::NotAbsolute),
        v1::ScanFolderProblem::NotDirectory => Some(ScanFolderProblem::NotDirectory),
        v1::ScanFolderProblem::NotUtf8 => Some(ScanFolderProblem::NotUtf8),
        v1::ScanFolderProblem::Unspecified => None,
    }
}

#[must_use]
pub fn encode_repository_failure(failure: &RepositoryFailure) -> v1::RepositoryFailure {
    use v1::repository_failure::Reason;
    let path = |path: &PathBuf| path.to_string_lossy().into_owned();
    let reason = match failure {
        RepositoryFailure::NotARepository { path: value } => {
            Reason::NotARepository(v1::RepositoryFailureNotARepository { path: path(value) })
        }
        RepositoryFailure::NoRepositories { root } => {
            Reason::NoRepositories(v1::RepositoryFailureNoRepositories { root: path(root) })
        }
        RepositoryFailure::SearchFailed {
            path: value,
            diagnostic,
        } => Reason::SearchFailed(v1::RepositoryFailureSearchFailed {
            path: path(value),
            diagnostic: diagnostic.to_string(),
        }),
    };
    v1::RepositoryFailure {
        reason: Some(reason),
    }
}

/// Returns `None` when the reason is absent or unknown to this build.
#[must_use]
pub fn decode_repository_failure(failure: v1::RepositoryFailure) -> Option<RepositoryFailure> {
    use v1::repository_failure::Reason;
    Some(match failure.reason? {
        Reason::NotARepository(value) => RepositoryFailure::NotARepository {
            path: PathBuf::from(value.path),
        },
        Reason::NoRepositories(value) => RepositoryFailure::NoRepositories {
            root: PathBuf::from(value.root),
        },
        Reason::SearchFailed(value) => RepositoryFailure::SearchFailed {
            path: PathBuf::from(value.path),
            diagnostic: ExternalDiagnostic::from(value.diagnostic),
        },
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        failure::{
            ErrorClass, ExternalDiagnostic, Failure, ProjectFailure, PushFailure, PushRefRejection,
            RejectedPushRef, RepositoryFailure, Resource, ScanFolderProblem, SettingsFailure,
            ViewerFailure,
        },
        paths::RepositoryRoot,
    };

    use super::{decode_failure, encode_failure};
    use crate::v1;

    fn failures() -> Vec<Failure> {
        vec![
            Failure::Unexpected,
            Failure::Unavailable,
            Failure::Busy,
            Failure::Changed,
            Failure::Gone {
                resource: Resource::PushOperation,
            },
            Failure::InvalidRequest {
                field: "identity".into(),
            },
            Failure::Push(PushFailure::NoUpstream {
                branch: "main".try_into().unwrap(),
            }),
            Failure::Push(PushFailure::Rejected {
                refs: vec![
                    RejectedPushRef {
                        destination: "refs/heads/main".try_into().unwrap(),
                        reason: PushRefRejection::FetchFirst,
                    },
                    RejectedPushRef {
                        destination: "refs/heads/release".try_into().unwrap(),
                        reason: PushRefRejection::RemoteRejected {
                            message: ExternalDiagnostic::new("protected branch"),
                        },
                    },
                ],
                diagnostic: ExternalDiagnostic::new("! [rejected] main -> main (fetch first)"),
            }),
            Failure::Settings(SettingsFailure::Invalid {
                path: "/home/user/.config/git-tools/config.toml".into(),
                diagnostic: ExternalDiagnostic::new("expected `=`"),
            }),
            Failure::Settings(SettingsFailure::Stale),
            Failure::Settings(SettingsFailure::Locked { wait_seconds: 5 }),
            Failure::Settings(SettingsFailure::PathUnavailable),
            Failure::Viewer(ViewerFailure::SourcePreparing),
            Failure::Viewer(ViewerFailure::RevealTooLarge),
            Failure::Viewer(ViewerFailure::SnapshotNameInvalid {
                characters_max: 200,
            }),
            Failure::Viewer(ViewerFailure::EditorFailed {
                diagnostic: ExternalDiagnostic::new("editor `code` was not found"),
            }),
            Failure::Viewer(ViewerFailure::FileOutsideRepository),
            Failure::Viewer(ViewerFailure::SourceNotRepository {
                path: "/work/repo".into(),
            }),
            Failure::Viewer(ViewerFailure::CommitFailed),
            Failure::Gone {
                resource: Resource::ViewerTab,
            },
            Failure::Project(ProjectFailure::ScanFolderInvalid {
                problem: ScanFolderProblem::NotUtf8,
            }),
            Failure::Project(ProjectFailure::NoCommonAncestor {
                path: RepositoryRoot::try_new("/work/repo".into()).unwrap(),
                branch: "main".to_owned().try_into().unwrap(),
                project: None,
            }),
            Failure::Project(ProjectFailure::ComparisonBranchMissing {
                path: RepositoryRoot::try_new("/work/repo-review".into()).unwrap(),
                branch: "develop".to_owned().try_into().unwrap(),
                project: Some(RepositoryRoot::try_new("/work/repo".into()).unwrap()),
            }),
            Failure::Project(ProjectFailure::CatalogueFull { projects_max: 4096 }),
            Failure::Repository(RepositoryFailure::SearchFailed {
                path: "/work".into(),
                diagnostic: ExternalDiagnostic::new("permission denied"),
            }),
        ]
    }

    #[test]
    fn known_failures_round_trip() {
        for failure in failures() {
            assert_eq!(decode_failure(encode_failure(&failure)), Some(failure));
        }
    }

    #[test]
    fn absent_reasons_and_relayed_unknown_reasons_decode_as_none() {
        assert_eq!(decode_failure(v1::Failure { reason: None }), None);
        assert_eq!(
            decode_failure(encode_failure(&Failure::Unrecognized {
                class: ErrorClass::Internal
            })),
            None
        );
    }

    #[test]
    fn malformed_parameters_decode_as_none() {
        let failure = v1::Failure {
            reason: Some(v1::failure::Reason::Push(v1::PushFailure {
                reason: Some(v1::push_failure::Reason::CommitRemoved(
                    v1::PushFailureCommitRemoved {
                        commit: "not-a-sha".into(),
                    },
                )),
            })),
        };

        assert_eq!(decode_failure(failure), None);
    }
}
