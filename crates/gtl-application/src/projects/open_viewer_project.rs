use gtl_models::{
    failure::{ErrorMeta, Failure, Resource},
    projects::ProjectRepository,
    recipes::RecipeBatchId,
};
use gtl_wire::viewer::projects::OpenViewerProject;

use crate::{
    ports::GitClient,
    recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget},
    viewer::{
        ViewerState,
        work::{self, ReservedRecipeWork},
    },
};

pub struct OpenProjectComparison {
    pub project: OpenViewerProject,
    pub repositories: Vec<ProjectRepository>,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum OpenViewerProjectError {
    #[error("project is no longer available")]
    #[meta(failure = Failure::Gone { resource: Resource::Project })]
    NotFound,
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// Opens a managed project's unpushed or branch changes as a snapshot tab.
#[cqrsy::command]
pub fn execute(
    request: OpenProjectComparison,
    git: &impl GitClient,
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
    work::reserve_open(
        viewer,
        Recipe {
            source: RecipeSource::LocalRepo {
                root: request.project.path,
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            },
            name: None,
        },
        RecipeBatchId::generate(),
    )
    .map_err(anyhow::Error::from)
    .map_err(Into::into)
}
