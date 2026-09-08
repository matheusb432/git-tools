use gtl_models::{projects::ProjectRepository, recipes::RecipeBatchId, viewer::ViewerTabKind};
use gtl_wire::viewer::projects::OpenViewerProject;
use rusqlite::Connection;

use crate::{
    live_views::{
        recipe_for_record,
        save_live_view::{self, SaveLiveView, SaveLiveViewOutcome},
    },
    ports::{Clock, GitClient},
    viewer::{
        ViewerState,
        work::{self, ReservedRecipeWork},
    },
};

pub struct OpenProjectComparison {
    pub project: OpenViewerProject,
    pub repositories: Vec<ProjectRepository>,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenViewerProjectError {
    #[error("project is no longer available")]
    NotFound,
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::command]
pub fn execute(
    request: OpenProjectComparison,
    git: &impl GitClient,
    connection: &mut Connection,
    clock: &impl Clock,
    viewer: &ViewerState,
) -> Result<ReservedRecipeWork, OpenViewerProjectError> {
    let mut matched = false;
    for repository in request.repositories {
        let path = if git.repo_present(&repository.path) {
            git.discover_top(repository.path.as_ref())
                .ok()
                .flatten()
                .unwrap_or(repository.path)
        } else {
            repository.path
        };
        if path == request.project.path {
            matched = true;
            break;
        }
    }
    if !matched {
        return Err(OpenViewerProjectError::NotFound);
    }
    let result = save_live_view::execute(
        SaveLiveView {
            path: request.project.path.as_ref().to_path_buf(),
            comparison: request.project.comparison,
        },
        git,
        connection,
        clock,
    )
    .map_err(anyhow::Error::from)?;
    let record = match result.outcome {
        SaveLiveViewOutcome::Created { record } | SaveLiveViewOutcome::Refreshed { record } => {
            record
        }
        SaveLiveViewOutcome::Rejected { .. } => return Err(OpenViewerProjectError::NotFound),
    };
    work::reserve_open(
        viewer,
        recipe_for_record(&record),
        RecipeBatchId::generate(),
        ViewerTabKind::Live,
    )
    .map_err(anyhow::Error::from)
    .map_err(Into::into)
}
