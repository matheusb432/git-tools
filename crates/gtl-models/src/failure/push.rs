use std::fmt;

use serde::{Deserialize, Serialize};

use super::{ErrorClass, ExternalDiagnostic, Failure, PublicFailure};
use crate::{
    diffs::{CommitId, CommitIdAbbreviation},
    git::{BranchName, GitRefName, RemoteName},
};

/// Why the viewer cannot prepare or complete a commit push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushFailure {
    /// The selection has no commits that the upstream lacks.
    NothingToPush,
    /// The checked-out branch has no remote upstream branch.
    NoUpstream { branch: BranchName },
    /// No branch is checked out.
    Detached,
    /// A different branch is checked out than the one reviewed.
    CheckoutChanged { current: BranchName },
    /// The selected commit is no longer in the branch history.
    CommitRemoved { commit: CommitId },
    /// The upstream remote, destination, or URL changed after review.
    DestinationChanged,
    /// The remote has more than one push URL, so an atomic push is impossible.
    MultipleDestinations { remote: RemoteName },
    /// The reviewed push was not started before its review lifetime ended.
    ReviewExpired,
    /// The server already retains its maximum number of push operations.
    HistoryFull { operations_max: u32 },
    /// The remote rejected at least one ref.
    Rejected {
        refs: Vec<RejectedPushRef>,
        diagnostic: ExternalDiagnostic,
    },
    /// Git failed for a reason other than a ref rejection.
    GitFailed { diagnostic: ExternalDiagnostic },
}

/// A destination ref that the remote refused, parsed from `git push --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedPushRef {
    pub destination: GitRefName,
    pub reason: PushRefRejection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushRefRejection {
    /// The remote has commits that the local branch lacks.
    FetchFirst,
    /// The update is not a fast-forward of the remote ref.
    NonFastForward,
    /// The remote refused the update, for example through a hook or branch protection.
    RemoteRejected { message: ExternalDiagnostic },
    /// Git reported a rejection this version does not classify.
    Other { summary: ExternalDiagnostic },
}

impl PushFailure {
    /// Verbatim Git output that explains the failure, when Git produced it.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&ExternalDiagnostic> {
        match self {
            Self::Rejected { diagnostic, .. } | Self::GitFailed { diagnostic } => Some(diagnostic),
            _ => None,
        }
    }

    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::HistoryFull { .. } => ErrorClass::ResourceExhausted,
            Self::GitFailed { .. } => ErrorClass::Internal,
            Self::NothingToPush
            | Self::NoUpstream { .. }
            | Self::Detached
            | Self::CheckoutChanged { .. }
            | Self::CommitRemoved { .. }
            | Self::DestinationChanged
            | Self::MultipleDestinations { .. }
            | Self::ReviewExpired
            | Self::Rejected { .. } => ErrorClass::FailedPrecondition,
        }
    }
}

impl PublicFailure for PushFailure {
    fn failure(&self) -> Failure {
        Failure::Push(self.clone())
    }
}

impl From<PushFailure> for Failure {
    fn from(failure: PushFailure) -> Self {
        Self::Push(failure)
    }
}

impl std::error::Error for PushFailure {}

impl fmt::Display for PushFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingToPush => {
                formatter.write_str("There are no unpushed commits through this SHA.")
            }
            Self::NoUpstream { branch } => write!(
                formatter,
                "Branch {branch} has no remote upstream. Configure its upstream before pushing."
            ),
            Self::Detached => formatter.write_str("Check out a branch before pushing."),
            Self::CheckoutChanged { current } => write!(
                formatter,
                "The checkout changed to {current}. Review the push again."
            ),
            Self::CommitRemoved { commit } => write!(
                formatter,
                "Commit {} is no longer in this branch's history. Select a current commit or open a new diff.",
                commit.abbreviated(CommitIdAbbreviation::TenCharacters)
            ),
            Self::DestinationChanged => {
                formatter.write_str("The upstream destination changed. Review the push again.")
            }
            Self::MultipleDestinations { remote } => write!(
                formatter,
                "Remote {remote} must have exactly one push URL for an atomic push."
            ),
            Self::ReviewExpired => {
                formatter.write_str("This push review expired. Review the push again.")
            }
            Self::HistoryFull { operations_max } => write!(
                formatter,
                "The server already tracks {operations_max} push operations. Try again after a push completes."
            ),
            Self::Rejected { refs, .. } => match refs.first().map(|rejected| &rejected.reason) {
                Some(PushRefRejection::FetchFirst | PushRefRejection::NonFastForward) => {
                    formatter.write_str(
                        "The remote has commits that this branch does not. Pull them before pushing.",
                    )
                }
                Some(PushRefRejection::RemoteRejected { message }) => {
                    write!(formatter, "The remote rejected the push: {message}")
                }
                Some(PushRefRejection::Other { .. }) | None => {
                    formatter.write_str("The remote rejected the push.")
                }
            },
            Self::GitFailed { .. } => formatter.write_str("Git could not complete the push."),
        }
    }
}
