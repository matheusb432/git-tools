use gtl_models::failure::{ErrorMeta, ViewerFailure};

use super::ViewerTabState;
use crate::{
    ports::{GitClient, GitRepositoryState},
    recipes::{Recipe, RecipeSource},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRecipe {
    pub recipe: Recipe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeRecipeOutcome {
    Ready,
    Broken { state: ViewerTabState },
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ProbeRecipeError {
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// Checks that the recipe's source directory is still a Git repository before computing it.
#[cqrsy::query]
pub fn execute(
    query: ProbeRecipe,
    git: &impl GitClient,
) -> Result<ProbeRecipeOutcome, ProbeRecipeError> {
    let RecipeSource::LocalRepo(path) = query.recipe.source;
    let failure = match git.probe_repository(&path)? {
        GitRepositoryState::Repository { .. } => return Ok(ProbeRecipeOutcome::Ready),
        GitRepositoryState::NotFound => ViewerFailure::SourceDirectoryMissing { path: path.into() },
        GitRepositoryState::NotARepository => {
            ViewerFailure::SourceNotRepository { path: path.into() }
        }
    };
    Ok(ProbeRecipeOutcome::Broken {
        state: ViewerTabState::Broken { failure },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::GitRepositoryState,
        recipes::RecipeOp,
        utils::{FakeGitClient, viewer::recipe},
        viewer::probe_recipe,
    };

    fn git(repository_state: GitRepositoryState) -> FakeGitClient {
        FakeGitClient {
            repository_state: Some(repository_state),
            ..Default::default()
        }
    }

    #[test]
    fn missing_source_is_broken() {
        let response = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
            },
            &git(GitRepositoryState::NotFound),
        )
        .unwrap();

        assert_eq!(
            response,
            ProbeRecipeOutcome::Broken {
                state: ViewerTabState::Broken {
                    failure: gtl_models::failure::ViewerFailure::SourceDirectoryMissing {
                        path: "/repos/project".into(),
                    },
                },
            }
        );
    }

    #[test]
    fn non_repository_source_is_broken() {
        let response = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
            },
            &git(GitRepositoryState::NotARepository),
        )
        .unwrap();

        assert_eq!(
            response,
            ProbeRecipeOutcome::Broken {
                state: ViewerTabState::Broken {
                    failure: gtl_models::failure::ViewerFailure::SourceNotRepository {
                        path: "/repos/project".into(),
                    },
                },
            }
        );
    }

    #[test]
    fn valid_source_is_ready() {
        let response = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
            },
            &git(GitRepositoryState::Repository {
                top_level: crate::utils::repository_root("/repos/project"),
            }),
        )
        .unwrap();

        assert_eq!(response, ProbeRecipeOutcome::Ready);
    }

    #[test]
    fn unexpected_probe_failure_is_returned() {
        let error = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
            },
            &FakeGitClient {
                repository_probe_error: Some("probe failed".into()),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "probe failed");
    }
}
