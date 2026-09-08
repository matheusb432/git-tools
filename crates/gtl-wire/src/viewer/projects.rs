use gtl_models::{
    live_views::LiveComparison,
    paths::{ProjectName, RepositoryRoot},
    repository::status::RepositoryStatus,
    timestamps::MachineTimestamp,
    viewer::ViewerTabId,
};
use serde::{Deserialize, Serialize};

pub const VIEWER_PROJECTS_MAX: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerProject {
    pub path: RepositoryRoot,
    pub name: ProjectName,
    pub status: RepositoryStatus,
    pub last_rendered_at: Option<MachineTimestamp>,
    pub comparison_branch: gtl_models::projects::comparison::ComparisonBranch,
    pub branch_comparison: ViewerProjectBranchComparison,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerProjectBranchComparison {
    Upstream,
    Branch {
        commits_ahead: gtl_models::git::CommitCount,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateViewerProject {
    pub path: RepositoryRoot,
    pub comparison_branch: super::FieldUpdate<gtl_models::projects::comparison::ComparisonBranch>,
    pub expected_comparison_branch: gtl_models::projects::comparison::ComparisonBranch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerProject {
    pub path: RepositoryRoot,
    pub comparison: LiveComparison,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerProjectOk {
    pub tab_id: ViewerTabId,
}

impl ViewerProject {
    #[must_use]
    pub fn review_class(&self) -> gtl_models::repository::status::StatusClass {
        use gtl_models::repository::status::{StatusChanges, StatusClass};
        match (&self.status, &self.branch_comparison) {
            (RepositoryStatus::Absent, _) => StatusClass::Absent,
            (
                RepositoryStatus::Present {
                    changes: StatusChanges::Unavailable,
                    ..
                },
                _,
            )
            | (_, ViewerProjectBranchComparison::Unavailable { .. }) => StatusClass::Warn,
            (
                RepositoryStatus::Present { changes, .. },
                ViewerProjectBranchComparison::Branch { commits_ahead },
            ) => {
                if changes.is_dirty() || commits_ahead.into_inner() > 0 {
                    StatusClass::Pending
                } else {
                    StatusClass::Clean
                }
            }
            (_, ViewerProjectBranchComparison::Upstream) => self.status.class(),
        }
    }
}
