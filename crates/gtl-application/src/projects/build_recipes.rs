//! Builds snapshot recipes for project repositories with unpushed commits.

use gtl_models::projects::ProjectRepository;

use crate::{
    ports::GitClient,
    projects::select_unpushed_repositories,
    recipes::{Recipe, RecipeOp},
};

/// Requests recipes for ahead repositories from an already-resolved project catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildProjectRecipes {
    pub repos: Vec<ProjectRepository>,
    pub operation: RecipeOp,
}

/// Reports a failure while selecting project repositories.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildProjectRecipesError {
    /// Project-repository selection failed.
    #[error(transparent)]
    Select(#[from] select_unpushed_repositories::SelectUnpushedRepositoriesError),
}

/// Builds one complete recipe per present project repository ahead of upstream.
///
/// # Errors
///
/// Returns [`BuildProjectRecipesError`] when repository selection fails.
#[cqrsy::query]
pub fn execute(
    query: BuildProjectRecipes,
    git: &impl GitClient,
) -> Result<Vec<Recipe>, BuildProjectRecipesError> {
    let selected = select_unpushed_repositories::execute(query.repos, git)?;
    Ok(selected
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
    use gtl_models::projects::ProjectRepository;

    use super::BuildProjectRecipes;
    use crate::{
        projects::build_recipes,
        recipes::{RecipeOp, RecipeSource, RecipeTarget},
        utils::ScriptedGitClient,
    };

    fn repo(name: &str) -> ProjectRepository {
        ProjectRepository {
            name: crate::utils::project_name(name),
            path: crate::utils::repository_root(&format!("/repos/{name}")),
            remote: Some(gtl_models::git::RemoteUrl::try_new("origin").unwrap()),
        }
    }

    #[test]
    fn project_build_selects_only_ahead_repositories_and_keeps_project_order() {
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

        let recipes = build_recipes::execute(
            BuildProjectRecipes {
                repos: vec![repo("api"), repo("clean"), repo("web")],
                operation: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
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
            recipes[0].source,
            RecipeSource::LocalRepo(crate::utils::repository_root("/real/api"))
        );
        assert_eq!(
            recipes[0].op,
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
            Ok(ScriptedGitClient::applied("/real/api\n")),
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
        )
        .unwrap();

        assert_eq!(
            recipes[0].op,
            RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None }
            }
        );
    }
}
