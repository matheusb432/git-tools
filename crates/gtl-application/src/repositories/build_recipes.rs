//! Builds snapshot recipes for every repository discovered under a root.

use std::path::PathBuf;

use gtl_models::repository::traversal::RepositoryTraversalScope;

use crate::{
    ports::GitClient,
    recipes::{Recipe, RecipeOp},
    repositories::find_repository_roots,
};

/// Requests recipes for every repository discovered under a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildRepositoryRecipes {
    pub root: PathBuf,
    pub operation: RecipeOp,
    pub scope: RepositoryTraversalScope,
}

/// Reports a failure while discovering recursive repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildRepositoryRecipesError {
    /// Recursive repository discovery or top-level resolution failed.
    #[error(transparent)]
    Discover(#[from] find_repository_roots::FindRepositoryRootsError),
}

/// Builds one complete recipe per repository discovered under `query.root`.
///
/// # Errors
///
/// Returns [`BuildRepositoryRecipesError`] when repository discovery or top-level resolution fails.
#[cqrsy::query]
pub fn execute(
    query: BuildRepositoryRecipes,
    git: &impl GitClient,
) -> Result<Vec<Recipe>, BuildRepositoryRecipesError> {
    let repos = find_repository_roots::execute(
        find_repository_roots::FindRepositoryRoots {
            root: query.root,
            scope: query.scope,
        },
        git,
    )?;
    Ok(repos
        .into_iter()
        .map(|repo| {
            crate::recipes::build_resolved(
                repo.path,
                query.operation.clone(),
                Some(repo.label),
                git,
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use gtl_models::repository::traversal::RepositoryTraversalScope;

    use super::BuildRepositoryRecipes;
    use crate::{
        recipes::{RecipeOp, RecipeSource, RecipeTarget},
        repositories::build_recipes,
        utils::{self, ScriptedGitClient},
    };

    fn repository_root() -> tempfile::TempDir {
        let temporary = tempfile::tempdir().unwrap();
        utils::make_repository(&temporary.path().join("api"));
        utils::make_repository(&temporary.path().join("web"));
        temporary
    }

    #[test]
    fn recursive_build_preserves_discovery_order_labels_and_last_target() {
        let temporary = repository_root();
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/real/api\n"),
            ScriptedGitClient::applied("/real/web\n"),
            ScriptedGitClient::applied("api-base\n"),
            ScriptedGitClient::applied("api-head\n"),
            ScriptedGitClient::applied("web-base\n"),
            ScriptedGitClient::applied("web-head\n"),
        ]);

        let recipes = build_recipes::execute(
            BuildRepositoryRecipes {
                root: temporary.path().to_path_buf(),
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(2).unwrap(),
                        pinned: None,
                    },
                },
                scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
            },
            &git,
        )
        .unwrap();

        assert_eq!(
            recipes
                .iter()
                .map(|recipe| recipe.name.as_ref().map(|name| name.as_str()))
                .collect::<Vec<_>>(),
            [Some("api"), Some("web")]
        );
        assert_eq!(
            recipes[0],
            crate::recipes::Recipe {
                source: RecipeSource::LocalRepo(crate::utils::repository_root("/real/api")),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Last {
                        count: NonZeroU32::new(2).unwrap(),
                        pinned: Some(crate::utils::pinned_range("api-base", "api-head",)),
                    }
                },
                name: Some(crate::utils::project_name("api")),
            }
        );
    }

    #[test]
    fn recursive_build_includes_worktrees_when_requested() {
        let temporary = repository_root();
        utils::make_linked_worktree(
            &temporary.path().join("api-worktree"),
            "/real/api/.git/worktrees/feature",
        );
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/real/api\n"),
            ScriptedGitClient::applied("/real/api-worktree\n"),
            ScriptedGitClient::applied("/real/web\n"),
        ]);

        let recipes = build_recipes::execute(
            BuildRepositoryRecipes {
                root: temporary.path().to_path_buf(),
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Base {
                        rev: crate::utils::git_revision("main"),
                    },
                },
                scope: RepositoryTraversalScope::IncludeLinkedWorktrees,
            },
            &git,
        )
        .unwrap();

        assert_eq!(
            recipes
                .iter()
                .map(|recipe| recipe.name.as_ref().map(|name| name.as_str()))
                .collect::<Vec<_>>(),
            [Some("api"), Some("api-worktree"), Some("web")]
        );
    }

    #[test]
    fn pin_resolution_failures_keep_repository_recipes_symbolic() {
        let temporary = repository_root();
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("/real/api\n")),
            Ok(ScriptedGitClient::applied("/real/web\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let recipes = build_recipes::execute(
            BuildRepositoryRecipes {
                root: temporary.path().to_path_buf(),
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
            },
            &git,
        )
        .unwrap();

        assert!(recipes.iter().all(|recipe| recipe.op
            == RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }));
    }
}
