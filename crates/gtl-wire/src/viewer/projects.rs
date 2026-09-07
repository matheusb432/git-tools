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
