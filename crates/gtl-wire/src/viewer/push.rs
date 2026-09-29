pub use gtl_models::viewer::ViewerPushId;
use gtl_models::{
    diffs::CommitId,
    failure::Failure,
    git::{BranchName, RemoteName, RemoteUrl},
    paths::{ProjectName, RepositoryRoot},
};
use serde::{Deserialize, Serialize};

use super::ViewerViewIdentity;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreateViewerPush {
    Project { path: RepositoryRoot },
    View { identity: ViewerViewIdentity },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerPushRequest {
    pub id: ViewerPushId,
}

/// Display-only projection. Execution accepts only the server-issued operation ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerPushPreview {
    pub repository: RepositoryRoot,
    pub project: Option<ProjectName>,
    pub branch: BranchName,
    pub remote_branch: BranchName,
    pub remote: RemoteName,
    pub remote_url: RemoteUrl,
    pub commit: CommitId,
    pub count: u64,
    pub no_confirmation: bool,
    pub command: String,
    pub command_arguments: Vec<ViewerPushCommandArgument>,
}

/// Server-declared command parts, in display order. The viewer supplies only their help text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerPushCommandArgument {
    Git,
    WorkingDirectory,
    DisableMirroring,
    Push,
    Atomic,
    Porcelain,
    NoFollowTags,
    NoRecurseSubmodules,
    OptionSeparator,
    Remote,
    CommitRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerPushStatus {
    Review(ViewerPushPreview),
    Queued,
    Running,
    Succeeded,
    Failed { failure: Failure },
}

/// Eligibility for the current selection, checked against the repository's current upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerPushAvailability {
    Available,
    NothingToPush,
    /// Git state prevents a push; the failure says why.
    Blocked {
        failure: Failure,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerPushState {
    pub availability: ViewerPushAvailability,
    pub snapshot_has_unpushed_commits: Option<bool>,
}
