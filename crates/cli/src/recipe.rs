//! Thin composition from CLI-resolved inputs to application recipe operations.

use std::path::Path;

use application::{
    diffs::DiffTarget,
    recipes::{
        RecipeRequest,
        build_managed::{self, BuildManagedRecipes},
        build_subrepos::{self, BuildSubrepoRecipes},
        pin::{self, PinRecipe},
    },
};
use domain::{discovery::DiscoveredRepo, managed::ManagedRepo};
use gtl_recipe::Recipe;

use crate::commands::managed::{self, ManagedOptions};

/// Mints a fresh batch identifier for recipes opened together.
pub(crate) fn new_batch_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Builds one recipe through the application slice with the production Git adapter.
pub(crate) fn recipe_for_cwd(
    repo: &Path,
    operation: RecipeRequest,
    name: Option<&str>,
) -> anyhow::Result<Recipe> {
    Ok(pin::execute(
        PinRecipe {
            repo: repo.to_path_buf(),
            operation,
            name: name.map(str::to_string),
        },
        &infra::git_runner::StdGitRunner,
    )?)
}

/// Loads the managed manifest and selects repositories with unpushed commits.
pub(crate) fn selected_managed_repos(
    options: &ManagedOptions,
) -> anyhow::Result<Vec<DiscoveredRepo>> {
    let repos = managed::load_repos(options)?;
    select_managed_repos(repos)
}

fn select_managed_repos(repos: Vec<ManagedRepo>) -> anyhow::Result<Vec<DiscoveredRepo>> {
    Ok(application::managed::select_unpushed::execute(
        application::managed::select_unpushed::SelectUnpushed { repos },
        &infra::git_runner::StdGitRunner,
    )?)
}

/// Builds managed snapshot recipes through the application slice.
pub(crate) fn managed_recipes(
    options: &ManagedOptions,
    operation: RecipeRequest,
) -> anyhow::Result<Vec<Recipe>> {
    let repos = managed::load_repos(options)?;
    Ok(build_managed::execute(
        BuildManagedRecipes { repos, operation },
        &infra::git_runner::StdGitRunner,
    )?)
}

/// Builds recursive snapshot recipes through the application slice.
pub(crate) fn subrepo_recipes(
    root: &Path,
    target: DiffTarget,
    include_worktrees: bool,
) -> anyhow::Result<Vec<Recipe>> {
    let root = std::fs::canonicalize(root)
        .map_err(|error| anyhow::anyhow!("failed to resolve {}: {error}", root.display()))?;
    Ok(build_subrepos::execute(
        BuildSubrepoRecipes {
            root,
            operation: RecipeRequest::Diff(target),
            include_worktrees,
        },
        &infra::repo_discovery::WalkdirRepoDiscovery,
        &infra::git_runner::StdGitRunner,
    )?)
}

#[cfg(test)]
mod tests {
    use super::new_batch_id;

    #[test]
    fn new_batch_id_yields_distinct_uuids() {
        assert_ne!(new_batch_id(), new_batch_id());
    }
}
