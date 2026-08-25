//! Builds one snapshot recipe from a repository path and a symbolic operation.

use std::path::PathBuf;

use gtl_models::paths::ProjectName;

use crate::{
    ports::GitClient,
    recipes::{Recipe, RecipeOp},
    repositories::resolve_repository_root,
};

/// Requests a resolved recipe for one repository path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRecipe {
    pub repo_path: PathBuf,
    pub operation: RecipeOp,
    pub name: Option<ProjectName>,
}

/// Reports that the repository root could not be resolved.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildRecipeError {
    #[error(transparent)]
    Resolve(#[from] resolve_repository_root::ResolveRepositoryRootError),
}

/// Resolves the repository root and pins any resolvable symbolic range.
///
/// # Errors
///
/// Returns [`BuildRecipeError`] when Git cannot resolve the repository path.
#[cqrsy::query]
pub fn execute(query: BuildRecipe, git: &impl GitClient) -> Result<Recipe, BuildRecipeError> {
    let repository_root = resolve_repository_root::execute(query.repo_path, git)?;
    Ok(super::build_resolved(
        repository_root,
        query.operation,
        query.name,
        git,
    ))
}
