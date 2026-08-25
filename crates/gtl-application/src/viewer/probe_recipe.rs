use gtl_models::live_views::LiveSource;

use super::{ViewerTabKind, ViewerTabState};
use crate::{
    live_views::probe_source::{self, ProbeOutcome},
    ports::GitClient,
    recipes::{Recipe, RecipeSource},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeRecipe {
    pub recipe: Recipe,
    pub kind: ViewerTabKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeRecipeOutcome {
    Ready,
    Broken { state: ViewerTabState },
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeRecipeError {
    #[error(transparent)]
    Probe(#[from] probe_source::ProbeSourceError),
}

#[cqrsy::query]
pub fn execute(
    query: ProbeRecipe,
    git: &impl GitClient,
) -> Result<ProbeRecipeOutcome, ProbeRecipeError> {
    if query.kind == ViewerTabKind::Snapshot {
        return Ok(ProbeRecipeOutcome::Ready);
    }

    let RecipeSource::LocalRepo(path) = query.recipe.source;
    let outcome = match probe_source::execute(LiveSource::local_repo(path), git)? {
        ProbeOutcome::Ok => ProbeRecipeOutcome::Ready,
        ProbeOutcome::Broken { rejection } => ProbeRecipeOutcome::Broken {
            state: ViewerTabState::Broken {
                code: rejection.code().into(),
                reason: rejection.to_string(),
            },
        },
    };

    Ok(outcome)
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
    fn snapshot_does_not_probe_its_source() {
        let response = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
                kind: ViewerTabKind::Snapshot,
            },
            &git(GitRepositoryState::NotFound),
        )
        .expect("snapshot probe policy succeeds");

        assert_eq!(response, ProbeRecipeOutcome::Ready);
    }

    #[test]
    fn missing_live_source_is_broken() {
        let response = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
                kind: ViewerTabKind::Live,
            },
            &git(GitRepositoryState::NotFound),
        )
        .expect("live probe succeeds");

        assert_eq!(
            response,
            ProbeRecipeOutcome::Broken {
                state: ViewerTabState::Broken {
                    code: "DirNotFound".into(),
                    reason: "The git repo's directory at `/repos/project` was not found.".into(),
                },
            }
        );
    }

    #[test]
    fn non_repository_live_source_is_broken() {
        let response = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
                kind: ViewerTabKind::Live,
            },
            &git(GitRepositoryState::NotARepository),
        )
        .expect("live probe succeeds");

        assert_eq!(
            response,
            ProbeRecipeOutcome::Broken {
                state: ViewerTabState::Broken {
                    code: "DirNotGitRepo".into(),
                    reason: "The directory `/repos/project` is not a git repository.".into(),
                },
            }
        );
    }

    #[test]
    fn valid_live_source_is_ready() {
        let response = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
                kind: ViewerTabKind::Live,
            },
            &git(GitRepositoryState::Repository {
                top_level: crate::utils::repository_root("/repos/project"),
            }),
        )
        .expect("live probe succeeds");

        assert_eq!(response, ProbeRecipeOutcome::Ready);
    }

    #[test]
    fn unexpected_live_probe_failure_is_returned() {
        let error = probe_recipe::execute(
            ProbeRecipe {
                recipe: recipe(RecipeOp::MergeDiff {
                    base: None,
                    pinned: None,
                }),
                kind: ViewerTabKind::Live,
            },
            &FakeGitClient {
                repository_probe_error: Some("probe failed".into()),
                ..Default::default()
            },
        )
        .expect_err("unexpected probe failure returns");

        assert_eq!(error.to_string(), "probe failed");
    }
}
