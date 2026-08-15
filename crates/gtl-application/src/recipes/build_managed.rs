//! Builds snapshot recipes for managed repositories with unpushed commits.

use gtl_models::managed::ManagedRepo;
use gtl_wire::recipes::{Recipe, RecipeOp};

use crate::{managed::select_unpushed, ports::GitClient};

/// Requests recipes for ahead repositories from an already-resolved project catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildManagedRecipes {
    pub repos: Vec<ManagedRepo>,
    pub operation: RecipeOp,
}

/// Reports a failure while selecting managed repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildManagedRecipesError {
    /// Managed-repository selection failed.
    #[error(transparent)]
    Select(#[from] select_unpushed::SelectUnpushedError),
}

/// Builds one complete recipe per present managed repository ahead of upstream.
///
/// # Errors
///
/// Returns [`BuildManagedRecipesError`] when repository selection fails.
#[cqrsy::query]
pub fn execute(
    query: BuildManagedRecipes,
    git: &impl GitClient,
) -> Result<Vec<Recipe>, BuildManagedRecipesError> {
    let selected =
        select_unpushed::execute(select_unpushed::SelectUnpushed { repos: query.repos }, git)?;
    Ok(selected
        .into_iter()
        .map(|repo| {
            super::build_resolved(repo.path, query.operation.clone(), Some(repo.label), git)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use gtl_models::managed::ManagedRepo;
    use gtl_wire::recipes::{RecipeOp, RecipeSource, RecipeTarget};

    use super::{BuildManagedRecipes, execute};
    use crate::testing::ScriptedGitClient;

    fn repo(name: &str) -> ManagedRepo {
        ManagedRepo {
            name: name.into(),
            path: PathBuf::from(format!("/repos/{name}")),
            remote: "origin".into(),
        }
    }

    #[test]
    fn managed_build_selects_only_ahead_repositories_and_keeps_project_order() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("2\n"),
            ScriptedGitClient::applied("/real/api\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("0\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("1\n"),
            ScriptedGitClient::applied("/real/web\n"),
            ScriptedGitClient::applied("api-base\n"),
            ScriptedGitClient::applied("api-head\n"),
            ScriptedGitClient::applied("web-base\n"),
            ScriptedGitClient::applied("web-head\n"),
        ]);

        let recipes = execute(
            BuildManagedRecipes {
                repos: vec![repo("api"), repo("clean"), repo("web")],
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            },
            &git,
        )
        .expect("managed recipes build");

        assert_eq!(
            recipes
                .iter()
                .map(|recipe| recipe.name.as_deref())
                .collect::<Vec<_>>(),
            [Some("api"), Some("web")]
        );
        assert_eq!(
            recipes[0].source,
            RecipeSource::LocalRepo("/real/api".into())
        );
        assert_eq!(
            recipes[0].op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(crate::testing::pinned_range("api-base", "api-head"))
                }
            }
        );
    }

    #[test]
    fn pin_resolution_failure_keeps_the_managed_recipe_symbolic() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("origin/main\n")),
            Ok(ScriptedGitClient::applied("1\n")),
            Ok(ScriptedGitClient::applied("/real/api\n")),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let recipes = execute(
            BuildManagedRecipes {
                repos: vec![repo("api")],
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            },
            &git,
        )
        .expect("pin failure is an optional optimization");

        assert_eq!(
            recipes[0].op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
    }
}
