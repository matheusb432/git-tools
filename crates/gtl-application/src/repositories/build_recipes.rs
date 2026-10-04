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

pub struct BuildRepositoryRecipesOk {
    pub recipes: Vec<Recipe>,
    pub notes: Vec<crate::shared::notes::Note>,
}

/// Reports a failure while discovering recursive repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildRepositoryRecipesError {
    #[error(transparent)]
    Comparison(#[from] crate::projects::comparison::ComparisonError),
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
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<BuildRepositoryRecipesOk, BuildRepositoryRecipesError> {
    let repos = find_repository_roots::execute(
        find_repository_roots::FindRepositoryRoots {
            root: query.root,
            scope: query.scope,
        },
        git,
    )?;
    let mut recipes = Vec::new();
    let mut notes = Vec::new();
    for repo in repos {
        if matches!(
            query.operation,
            RecipeOp::Diff {
                target: crate::recipes::RecipeTarget::Unpushed { pinned: None }
            }
        ) {
            match crate::projects::comparison::resolve(&repo.path, git, comparisons) {
                Ok(_) => {}
                Err(error) if error.is_unavailable() => {
                    notes.push(crate::shared::notes::Note::warn(format!(
                        "{}: {error}",
                        repo.label
                    )));
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
        }
        recipes.push(crate::recipes::build_resolved(
            repo.path,
            query.operation.clone(),
            Some(repo.label),
            git,
        ));
    }
    Ok(BuildRepositoryRecipesOk { recipes, notes })
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
            ScriptedGitClient::applied("//fixture.invalid/repositories/real/api\n"),
            ScriptedGitClient::applied("//fixture.invalid/repositories/real/web\n"),
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
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap()
        .recipes;

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
                source: RecipeSource::LocalRepo {
                    root: crate::utils::repository_root("//fixture.invalid/repositories/real/api"),
                    op: RecipeOp::Diff {
                        target: RecipeTarget::Last {
                            count: NonZeroU32::new(2).unwrap(),
                            pinned: Some(crate::utils::pinned_range("api-base", "api-head",)),
                        }
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
            &temporary.path().join("api/.git/worktrees/feature"),
        );
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("//fixture.invalid/repositories/real/api\n"),
            ScriptedGitClient::applied("//fixture.invalid/repositories/real/api-worktree\n"),
            ScriptedGitClient::applied("//fixture.invalid/repositories/real/web\n"),
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
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap()
        .recipes;

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
            Ok(ScriptedGitClient::applied(
                "//fixture.invalid/repositories/real/api\n",
            )),
            Ok(ScriptedGitClient::applied(
                "//fixture.invalid/repositories/real/web\n",
            )),
            Ok(ScriptedGitClient::applied("origin/main\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
            Ok(ScriptedGitClient::applied("origin/main\n")),
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
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap()
        .recipes;

        assert!(recipes.iter().all(|recipe| recipe.op().cloned().unwrap()
            == RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }));
    }
}
