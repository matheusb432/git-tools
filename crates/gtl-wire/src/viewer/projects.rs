use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    repository::status::RepositoryStatus,
    timestamps::MachineTimestamp,
    viewer::ViewerTabId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverProjectRepositories {
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDiscovery {
    pub root: String,
    pub repositories: Vec<DiscoveredProjectRepository>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredProjectRepository {
    pub path: RepositoryRoot,
    pub label: ProjectName,
    pub state: ProjectDiscoveryState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectDiscoveryState {
    New,
    Active(gtl_models::projects::catalogue::ProjectId),
    Paused(gtl_models::projects::catalogue::ProjectId),
    Unmanaged(gtl_models::projects::catalogue::ProjectId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportProjectRepositories {
    pub selections: Vec<ProjectImportSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectImportSelection {
    pub path: String,
    pub project_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectImportResult {
    pub path: String,
    pub project_id: String,
    pub outcome: ProjectImportOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectImportOutcome {
    Created,
    Restored,
    Failed(gtl_models::failure::Failure),
}

#[nutype::nutype(
    validate(greater_or_equal = 1, less_or_equal = 100),
    default = 15,
    derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)
)]
pub struct ViewerProjectsPageSize(u32);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerProjectsCursor {
    #[default]
    First,
    After(gtl_models::projects::catalogue::ProjectId),
    Before(gtl_models::projects::catalogue::ProjectId),
    Last,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListViewerProjects {
    pub cursor: ViewerProjectsCursor,
    pub page_size: ViewerProjectsPageSize,
    pub sort: Option<gtl_models::settings::ProjectsSort>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ViewerProjectPageData")]
pub struct ViewerProjectPage {
    projects: Vec<ViewerProject>,
    total: u32,
    count_before: u32,
}

#[derive(Deserialize)]
struct ViewerProjectPageData {
    projects: Vec<ViewerProject>,
    total: u32,
    count_before: u32,
}

impl TryFrom<ViewerProjectPageData> for ViewerProjectPage {
    type Error = &'static str;

    fn try_from(data: ViewerProjectPageData) -> Result<Self, Self::Error> {
        Self::try_new(data.projects, data.total, data.count_before).ok_or("invalid project page")
    }
}

impl ViewerProjectPage {
    #[must_use]
    pub fn try_new(projects: Vec<ViewerProject>, total: u32, count_before: u32) -> Option<Self> {
        let length = u32::try_from(projects.len()).ok()?;
        if length > 100
            || count_before.checked_add(length)? > total
            || projects
                .iter()
                .map(|project| &project.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != projects.len()
        {
            return None;
        }
        Some(Self {
            projects,
            total,
            count_before,
        })
    }

    #[must_use]
    pub fn projects(&self) -> &[ViewerProject] {
        &self.projects
    }

    #[must_use]
    pub const fn total(&self) -> u32 {
        self.total
    }

    #[must_use]
    pub const fn count_before(&self) -> u32 {
        self.count_before
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetViewerProjectStatus {
    pub project_id: gtl_models::projects::catalogue::ProjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerProjectStatus {
    pub project_id: gtl_models::projects::catalogue::ProjectId,
    pub status: RepositoryStatus,
    pub comparison_branch: gtl_models::projects::comparison::ComparisonBranch,
    pub branch_comparison: ViewerProjectBranchComparison,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewerProject {
    pub id: gtl_models::projects::catalogue::ProjectId,
    pub path: RepositoryRoot,
    pub name: ProjectName,
    pub last_rendered_at: Option<MachineTimestamp>,
    pub comparison_branch: gtl_models::projects::comparison::ComparisonBranch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerProjectBranchComparison {
    Upstream,
    Branch {
        commits_ahead: gtl_models::git::CommitCount,
    },
    Unavailable {
        failure: gtl_models::failure::Failure,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateViewerProject {
    pub path: RepositoryRoot,
    pub comparison_branch: super::FieldUpdate<gtl_models::projects::comparison::ComparisonBranch>,
    pub expected_comparison_branch: gtl_models::projects::comparison::ComparisonBranch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerProjectDiffMode {
    Snapshot,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerProject {
    pub path: RepositoryRoot,
    pub mode: ViewerProjectDiffMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenViewerProjectOk {
    pub tab_id: ViewerTabId,
}

impl ViewerProjectStatus {
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "Vec<gtl_models::projects::catalogue::ProjectId>",
    into = "Vec<gtl_models::projects::catalogue::ProjectId>"
)]
pub struct ViewerProjectSelection(Vec<gtl_models::projects::catalogue::ProjectId>);

impl TryFrom<Vec<gtl_models::projects::catalogue::ProjectId>> for ViewerProjectSelection {
    type Error = &'static str;

    fn try_from(ids: Vec<gtl_models::projects::catalogue::ProjectId>) -> Result<Self, Self::Error> {
        if ids.len() > 100 || !ids.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err("project selection must contain at most 100 ordered distinct IDs");
        }
        Ok(Self(ids))
    }
}

impl From<ViewerProjectSelection> for Vec<gtl_models::projects::catalogue::ProjectId> {
    fn from(selection: ViewerProjectSelection) -> Self {
        selection.0
    }
}

impl ViewerProjectSelection {
    #[must_use]
    pub fn ids(&self) -> &[gtl_models::projects::catalogue::ProjectId] {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerProjectStatusUpdate {
    Status(ViewerProjectStatus),
    Unavailable(gtl_models::projects::catalogue::ProjectId),
}
