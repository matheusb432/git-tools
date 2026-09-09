//! Git operations whose repository scope is the active GTL project catalog.

use gtl_models::{paths::ProjectName, projects::ProjectRepository, settings::PushAllExclusions};

pub mod build_recipes;
pub mod catalogue;
pub mod commit_repositories;
pub mod comparison;
pub mod get_project_comparison_branch;
pub mod list_viewer_projects;
pub mod open_viewer_project;
pub mod plan_push;
pub mod pull_repositories;
pub mod push_repositories;
pub mod record_project_render;
mod remote_sync;
pub mod render_project_diff;
pub mod select_comparison_repositories;
pub mod update_project_comparison;
pub mod update_viewer_project;

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
