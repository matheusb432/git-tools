//! Typed public failure reasons and the classification that selects them.
//!
//! A [`Failure`] is the only failure data that crosses a process boundary. Callers branch on its
//! variants and render its text at their own edge; they never parse or display server prose.
//! Application errors declare their classification with [`ErrorMeta`], and transports map the
//! resulting [`ErrorClass`] and [`Failure`] once.

mod diagnostic;
mod project;
mod push;
mod repository;
mod settings;
mod viewer;

use std::fmt;

pub use diagnostic::ExternalDiagnostic;
pub use gtl_macros::ErrorMeta;
pub use project::{ProjectFailure, ScanFolderProblem};
pub use push::{PushFailure, PushRefRejection, RejectedPushRef};
pub use repository::RepositoryFailure;
use serde::{Deserialize, Serialize};
pub use settings::SettingsFailure;
pub use viewer::ViewerFailure;

/// Protocol-neutral failure category; transports map it to their own status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ErrorClass {
    Cancelled,
    InvalidArgument,
    DeadlineExceeded,
    NotFound,
    AlreadyExists,
    PermissionDenied,
    ResourceExhausted,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    Internal,
    Unavailable,
    DataLoss,
    Unauthenticated,
}

/// How an error crosses a process boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// The caller can act on this typed reason.
    Public(Failure),
    /// Only the category crosses; the error itself stays in server diagnostics.
    Private(ErrorClass),
}

impl Classification {
    /// The failure a caller receives: the public reason, or the generic reason for a private class.
    #[must_use]
    pub fn into_failure(self) -> Failure {
        match self {
            Self::Public(failure) => failure,
            Self::Private(class) => Failure::private(class),
        }
    }
}

/// Classifies an error at a process boundary. Derive it with [`ErrorMeta`].
pub trait Classified {
    fn classify(&self) -> Classification;
}

/// A typed reason that becomes a public [`Failure`].
pub trait PublicFailure {
    fn failure(&self) -> Failure;
}

impl<T: PublicFailure> Classified for T {
    fn classify(&self) -> Classification {
        Classification::Public(self.failure())
    }
}

/// A resource that an operation addressed but that no longer exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    PushOperation,
    ViewerTab,
    Commit,
    DiffFile,
    SourceRange,
    Snapshot,
    Project,
}

impl fmt::Display for Resource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PushOperation => "This push operation",
            Self::ViewerTab => "This viewer tab",
            Self::Commit => "This commit",
            Self::DiffFile => "This diff file",
            Self::SourceRange => "This source range",
            Self::Snapshot => "This snapshot",
            Self::Project => "This project",
        })
    }
}

/// A typed public failure reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// The server failed unexpectedly; details stay in its log.
    Unexpected,
    /// A dependency is temporarily unavailable.
    Unavailable,
    /// The server is at capacity for this kind of work.
    Busy,
    /// State changed while the operation ran.
    Changed,
    /// The addressed resource no longer exists.
    Gone {
        resource: Resource,
    },
    /// The request was malformed; `field` names the rejected input.
    InvalidRequest {
        field: String,
    },
    Push(PushFailure),
    Settings(SettingsFailure),
    Viewer(ViewerFailure),
    Project(ProjectFailure),
    Repository(RepositoryFailure),
    /// The peer sent a reason this build does not recognize.
    Unrecognized {
        class: ErrorClass,
    },
}

impl Failure {
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::Unexpected => ErrorClass::Internal,
            Self::Unavailable => ErrorClass::Unavailable,
            Self::Busy => ErrorClass::ResourceExhausted,
            Self::Changed => ErrorClass::Aborted,
            Self::Gone { .. } => ErrorClass::NotFound,
            Self::InvalidRequest { .. } => ErrorClass::InvalidArgument,
            Self::Push(failure) => failure.class(),
            Self::Settings(failure) => failure.class(),
            Self::Viewer(failure) => failure.class(),
            Self::Project(failure) => failure.class(),
            Self::Repository(failure) => failure.class(),
            Self::Unrecognized { class } => *class,
        }
    }

    /// Verbatim external output that explains the failure, shown apart from the headline.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::Push(failure) => failure.diagnostic(),
            Self::Settings(SettingsFailure::Invalid { diagnostic, .. }) => Some(diagnostic),
            Self::Viewer(failure) => failure.diagnostic(),
            Self::Project(failure) => failure.diagnostic(),
            Self::Repository(failure) => failure.diagnostic(),
            _ => None,
        }
    }

    /// The generic reason reported for a private error of `class`.
    #[must_use]
    pub const fn private(class: ErrorClass) -> Self {
        match class {
            ErrorClass::Unavailable | ErrorClass::DeadlineExceeded => Self::Unavailable,
            ErrorClass::ResourceExhausted => Self::Busy,
            ErrorClass::Aborted => Self::Changed,
            _ => Self::Unexpected,
        }
    }
}

impl PublicFailure for Failure {
    fn failure(&self) -> Failure {
        self.clone()
    }
}

impl std::error::Error for Failure {}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unexpected => formatter.write_str(
                "The server could not complete this action. Details are in the gtl-server log.",
            ),
            Self::Unavailable => {
                formatter.write_str("A required service is temporarily unavailable. Try again.")
            }
            Self::Busy => formatter.write_str("The server is busy. Try again shortly."),
            Self::Changed => {
                formatter.write_str("State changed while this action was running. Try again.")
            }
            Self::Gone { resource } => write!(formatter, "{resource} is no longer available."),
            Self::InvalidRequest { field } => {
                write!(formatter, "The request has an invalid `{field}` value.")
            }
            Self::Push(failure) => failure.fmt(formatter),
            Self::Settings(failure) => failure.fmt(formatter),
            Self::Viewer(failure) => failure.fmt(formatter),
            Self::Project(failure) => failure.fmt(formatter),
            Self::Repository(failure) => failure.fmt(formatter),
            Self::Unrecognized { class } => write!(
                formatter,
                "The server reported a {class} failure that this version does not recognize."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorClass, Failure, PushFailure};

    #[test]
    fn derive_resolves_the_facade_inside_its_own_crate() {
        use super::{Classification, Classified, ErrorMeta};

        #[derive(ErrorMeta)]
        enum Error {
            #[meta(private(Internal))]
            Private,
        }

        assert_eq!(
            Error::Private.classify(),
            Classification::Private(ErrorClass::Internal)
        );
    }

    #[test]
    fn private_classes_select_generic_reasons() {
        assert_eq!(Failure::private(ErrorClass::Internal), Failure::Unexpected);
        assert_eq!(
            Failure::private(ErrorClass::Unavailable),
            Failure::Unavailable
        );
        assert_eq!(
            Failure::private(ErrorClass::ResourceExhausted),
            Failure::Busy
        );
        assert_eq!(Failure::private(ErrorClass::Aborted), Failure::Changed);
    }

    #[test]
    fn feature_reasons_own_their_class() {
        assert_eq!(
            Failure::Push(PushFailure::Detached).class(),
            ErrorClass::FailedPrecondition
        );
        assert_eq!(
            Failure::Unrecognized {
                class: ErrorClass::DataLoss
            }
            .class(),
            ErrorClass::DataLoss
        );
    }

    #[test]
    fn failures_round_trip_through_serde() {
        for failure in [
            Failure::Unexpected,
            Failure::Push(PushFailure::NothingToPush),
            Failure::InvalidRequest {
                field: "identity".into(),
            },
        ] {
            let encoded = serde_json::to_string(&failure).unwrap();
            assert_eq!(serde_json::from_str::<Failure>(&encoded).unwrap(), failure);
        }
    }
}
