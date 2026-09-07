//! Git operations whose repository scope is the active sample_project project catalog.

use gtl_models::{paths::ProjectName, projects::ProjectRepository, settings::PushAllExclusions};

pub mod build_recipes;
pub mod commit_repositories;
pub mod list_viewer_projects;
pub mod open_viewer_project;
pub mod plan_push;
pub mod pull_repositories;
pub mod push_repositories;
pub mod record_project_render;
mod remote_sync;
pub mod render_project_diff;
pub mod select_unpushed_repositories;

pub use remote_sync::{RepoSyncResult, SyncExit, SyncStatus};

struct PushAllRepositorySelection {
    selected: Vec<ProjectRepository>,
    excluded: Vec<ProjectName>,
}

fn select_push_all_repositories(
    repositories: Vec<ProjectRepository>,
    exclusions: &PushAllExclusions,
) -> PushAllRepositorySelection {
    let mut selected = Vec::with_capacity(repositories.len());
    let mut excluded = Vec::new();
    for repository in repositories {
        if exclusions.contains(&repository.name) {
            excluded.push(repository.name);
        } else {
            selected.push(repository);
        }
    }
    PushAllRepositorySelection { selected, excluded }
}
