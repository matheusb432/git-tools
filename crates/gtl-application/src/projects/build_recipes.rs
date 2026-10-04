//! Builds snapshot recipes for project repositories with commits to compare.

use gtl_models::projects::ProjectRepository;

use crate::{
    ports::GitClient,
    projects::select_comparison_repositories,
    recipes::{Recipe, RecipeOp},
};

/// Requests recipes for ahead repositories from an already-resolved project catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildProjectRecipes {
    pub repos: Vec<ProjectRepository>,
    pub operation: RecipeOp,
}

pub struct BuildProjectRecipesOk {
    pub recipes: Vec<Recipe>,
    pub notes: Vec<crate::shared::notes::Note>,
}

/// Reports a failure while selecting project repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildProjectRecipesError {
    /// Project-repository selection failed.
    #[error(transparent)]
    Select(#[from] select_comparison_repositories::SelectComparisonRepositoriesError),
}

/// Builds one complete recipe per project repository ahead of its effective comparison.
///
/// # Errors
///
/// Returns [`BuildProjectRecipesError`] when repository selection fails.
#[cqrsy::query]
pub fn execute(
    query: BuildProjectRecipes,
    git: &impl GitClient,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<BuildProjectRecipesOk, BuildProjectRecipesError> {
    let selected = select_comparison_repositories::execute(query.repos, git, comparisons)?;
    let recipes = selected
        .repositories
        .into_iter()
        .map(|repo| {
            crate::recipes::build_resolved(
                repo.path,
                query.operation.clone(),
                Some(repo.label),
                git,
            )
        })
        .collect();
    Ok(BuildProjectRecipesOk {
        recipes,
        notes: selected.notes,
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::projects::ProjectRepository;

    use super::BuildProjectRecipes;
    use crate::{
        projects::build_recipes,
        recipes::{RecipeOp, RecipeTarget},
        utils::ScriptedGitClient,
    };

    fn repo(name: &str) -> ProjectRepository {
        ProjectRepository {
            name: crate::utils::project_name(name),
            path: crate::utils::repository_root(&format!(
                "//fixture.invalid/repositories/repos/{name}"
            )),
            remote: Some(gtl_models::git::RemoteUrl::try_new("origin").unwrap()),
        }
    }

    #[test]
    fn project_build_selects_only_ahead_repositories_and_keeps_project_order() {
        let git = ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("2\n"),
            ScriptedGitClient::applied("//fixture.invalid/repositories/real/api\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("0\n"),
            ScriptedGitClient::applied("origin/main\n"),
            ScriptedGitClient::applied("1\n"),
            ScriptedGitClient::applied("//fixture.invalid/repositories/real/web\n"),
            ScriptedGitClient::applied("api-base\n"),
            ScriptedGitClient::applied("api-head\n"),
            ScriptedGitClient::applied("web-base\n"),
            ScriptedGitClient::applied("web-head\n"),
        ]);

        let recipes = build_recipes::execute(
            BuildProjectRecipes {
                repos: vec![repo("api"), repo("clean"), repo("web")],
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
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
            recipes[0].cwd(),
            Some(&crate::utils::repository_root(
                "//fixture.invalid/repositories/real/api"
            ))
        );
        assert_eq!(
            recipes[0].op().cloned().unwrap(),
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(crate::utils::pinned_range("api-base", "api-head"))
                }
            }
        );
    }

    #[test]
    fn pin_resolution_failure_keeps_the_project_recipe_symbolic() {
        let git = ScriptedGitClient::with_results(vec![
            Ok(ScriptedGitClient::applied("origin/main\n")),
            Ok(ScriptedGitClient::applied("1\n")),
            Ok(ScriptedGitClient::applied(
                "//fixture.invalid/repositories/real/api\n",
            )),
            Err(anyhow::anyhow!("git transport unavailable")),
        ]);

        let recipes = build_recipes::execute(
            BuildProjectRecipes {
                repos: vec![repo("api")],
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
            },
            &git,
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap()
        .recipes;

        assert_eq!(
            recipes[0].op().cloned().unwrap(),
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
    }
}
