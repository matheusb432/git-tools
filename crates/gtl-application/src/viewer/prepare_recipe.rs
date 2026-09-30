//! The viewer recipe lifecycle: probe, compute, and choose the publication.

use std::sync::Arc;

use gtl_models::{failure::ErrorMeta, recipes::RecipeLabel, timestamps::MachineTimestamp};

use super::{
    ViewerTabState,
    complete_recipe_computation::{self, CompleteRecipeComputation, CompleteRecipeComputationOk},
    compute_recipe,
    probe_recipe::{self, ProbeRecipe, ProbeRecipeOutcome},
};
use crate::{
    diffs::View,
    history::record_render::RecordRender,
    ports::{GitClient, UserSettingsReader},
    recipes::{Recipe, RecipeLabelParts},
};

/// Requests one complete viewer recipe preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrepareRecipe {
    pub comparison_name: Option<gtl_models::git::GitRevision>,
    pub recipe: Recipe,
    pub changes_since: Option<MachineTimestamp>,
}

/// The application decision the process root applies to its session.
#[derive(Debug, Clone, PartialEq)]
pub enum PrepareRecipeOk {
    Broken {
        state: ViewerTabState,
    },
    Publish {
        label: RecipeLabel,
        view: Arc<View>,
        history: Box<RecordRender>,
    },
}

/// Failure while probing or computing a recipe.
#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum PrepareRecipeError {
    #[error(transparent)]
    #[meta(transparent)]
    Probe(#[from] probe_recipe::ProbeRecipeError),
    #[error(transparent)]
    #[meta(transparent)]
    Compute(#[from] compute_recipe::ComputeRecipeError),
}

/// Probes the source, pins unpushed commits, computes the view, and decides whether the result
/// is broken or publishable content.
#[cqrsy::query]
pub fn execute(
    query: PrepareRecipe,
    user_settings: &impl UserSettingsReader,
    git: &impl GitClient,
    filters: &impl crate::ports::ExtensionFilterReader,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<PrepareRecipeOk, PrepareRecipeError> {
    let PrepareRecipe {
        mut recipe,
        mut comparison_name,
        changes_since,
    } = query;
    let probe = probe_recipe::execute(
        ProbeRecipe {
            recipe: recipe.clone(),
        },
        git,
    )?;
    if let ProbeRecipeOutcome::Broken { state } = probe {
        return Ok(PrepareRecipeOk::Broken { state });
    }

    if let crate::recipes::RecipeOp::Diff {
        target: crate::recipes::RecipeTarget::Unpushed { pinned: None },
    } = &recipe.op
    {
        let comparison = crate::projects::comparison::resolve(&recipe.cwd(), git, comparisons)
            .map_err(crate::diffs::compute_diff::ComputeDiffError::from)
            .map_err(compute_recipe::ComputeRecipeError::from)?;
        let pin = comparison
            .pin(&recipe.cwd(), git)
            .map_err(crate::diffs::compute_diff::ComputeDiffError::from)
            .map_err(compute_recipe::ComputeRecipeError::from)?;
        comparison_name = Some(comparison.name());
        recipe.op = crate::recipes::RecipeOp::Diff {
            target: crate::recipes::RecipeTarget::Unpushed { pinned: Some(pin) },
        };
    }
    let mut view = compute_recipe::execute(
        compute_recipe::ComputeRecipe {
            recipe: recipe.clone(),
            changes_since,
        },
        user_settings,
        git,
        filters,
        comparisons,
    )?;
    if let Some(name) = &comparison_name {
        // The titlebar names the compared branch as the tab label does, not the pinned commit.
        view.upstream = name.clone();
    }
    let completed = complete_recipe_computation::execute(CompleteRecipeComputation {
        recipe: recipe.clone(),
        view,
        comparison_name: comparison_name.clone(),
    });
    let CompleteRecipeComputationOk { label, view } = completed;
    let history = Box::new(RecordRender {
        comparison_name,
        label_parts: RecipeLabelParts::from_view(&recipe, &view),
        recipe,
        repo_name: view.repo_name.clone(),
        range_label: view.cmd.range.clone(),
    });
    Ok(PrepareRecipeOk::Publish {
        label,
        view,
        history,
    })
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
            top_level: Some("//fixture.invalid/repositories/repos/project".into()),
            branch: "feature".into(),
            upstream: Some("main".into()),
            ..Default::default()
        }
    }

    #[test]
    fn missing_source_stops_before_computation() {
        let response = prepare_recipe::execute(
            PrepareRecipe {
                comparison_name: None,
                changes_since: None,
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                }),
            },
            &FixedUserSettingsStore::default(),
            &FakeGitClient {
                repository_state: Some(GitRepositoryState::NotFound),
                ..Default::default()
            },
            &crate::utils::SavedExtensionFilters::default(),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert!(matches!(response, PrepareRecipeOk::Broken { .. }));
    }

    #[test]
    fn ready_recipe_returns_a_publish_decision_for_its_pinned_commits() {
        let response = prepare_recipe::execute(
            PrepareRecipe {
                comparison_name: None,
                changes_since: None,
                recipe: recipe(RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                }),
            },
            &FixedUserSettingsStore::default(),
            &source(),
            &crate::utils::SavedExtensionFilters::default(),
            &crate::utils::ProjectComparisons::default(),
        )
        .unwrap();

        assert!(matches!(
            response,
            PrepareRecipeOk::Publish { view, history, .. }
                if history.recipe.is_pinned() && view.upstream.as_ref() == "main"
        ));
    }
}
