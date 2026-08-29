//! The viewer recipe lifecycle: probe, compute, and choose the publication.

use std::sync::Arc;

use super::{
    ViewerTabKind, ViewerTabState,
    complete_recipe_computation::{self, CompleteRecipeComputation, CompleteRecipeComputationOk},
    compute_recipe,
    probe_recipe::{self, ProbeRecipe, ProbeRecipeOutcome},
};
use crate::{
    diffs::View,
    history::record_render::RecordRender,
    ports::{GitClient, UserSettingsStore},
    recipes::Recipe,
};

/// Requests one complete viewer recipe preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareRecipe {
    pub recipe: Recipe,
    pub kind: ViewerTabKind,
}

/// The application decision the process root applies to its session.
#[derive(Debug, Clone, PartialEq)]
pub enum PrepareRecipeOk {
    Broken {
        state: ViewerTabState,
    },
    Skipped {
        label: String,
    },
    Publish {
        label: String,
        view: Arc<View>,
        history: RecordRender,
    },
}

/// Failure while probing or computing a recipe.
#[derive(Debug, thiserror::Error)]
pub enum PrepareRecipeError {
    #[error(transparent)]
    Probe(#[from] probe_recipe::ProbeRecipeError),
    #[error(transparent)]
    Compute(#[from] compute_recipe::ComputeRecipeError),
}

/// Probes the source, computes the view, and decides whether the result is
/// broken, an empty snapshot, or publishable content.
#[cqrsy::query]
pub fn execute(
    query: PrepareRecipe,
    user_settings: &impl UserSettingsStore,
    git: &impl GitClient,
) -> Result<PrepareRecipeOk, PrepareRecipeError> {
    let PrepareRecipe { recipe, kind } = query;
    let probe = probe_recipe::execute(
        ProbeRecipe {
            recipe: recipe.clone(),
            kind,
        },
        git,
    )?;
    if let ProbeRecipeOutcome::Broken { state } = probe {
        return Ok(PrepareRecipeOk::Broken { state });
    }

    let view = compute_recipe::execute(recipe.clone(), user_settings, git)?;
    let completed = complete_recipe_computation::execute(CompleteRecipeComputation {
        recipe: recipe.clone(),
        kind,
        view,
    });
    match completed {
        CompleteRecipeComputationOk::Skipped { label } => Ok(PrepareRecipeOk::Skipped { label }),
        CompleteRecipeComputationOk::Publish { label, view } => {
            let history = RecordRender {
                recipe,
                title: label.clone(),
                repo_name: view.repo_name.clone(),
                range_label: view.cmd.range.clone(),
            };
            Ok(PrepareRecipeOk::Publish {
                label,
                view,
                history,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ports::GitRepositoryState,
        recipes::{RecipeOp, RecipeTarget},
        utils::{FakeGitClient, FixedUserSettingsStore, viewer::recipe},
        viewer::prepare_recipe,
    };

    fn source() -> FakeGitClient {
        FakeGitClient {
            top_level: Some("/repos/project".into()),
            branch: "feature".into(),
            upstream: Some("main".into()),
            ..Default::default()
        }
    }

    #[test]
    fn missing_live_source_stops_before_computation() {
        let response = prepare_recipe::execute(
            PrepareRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                }),
                kind: ViewerTabKind::Live,
            },
            &FixedUserSettingsStore::default(),
            &FakeGitClient {
                repository_state: Some(GitRepositoryState::NotFound),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(matches!(response, PrepareRecipeOk::Broken { .. }));
    }

    #[test]
    fn ready_live_recipe_returns_a_publish_decision() {
        let response = prepare_recipe::execute(
            PrepareRecipe {
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                }),
                kind: ViewerTabKind::Live,
            },
            &FixedUserSettingsStore::default(),
            &source(),
        )
        .unwrap();

        assert!(matches!(response, PrepareRecipeOk::Publish { .. }));
    }
}
