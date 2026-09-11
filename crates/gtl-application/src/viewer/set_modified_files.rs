use std::sync::Arc;

use gtl_wire::viewer::SetViewerModifiedFiles;

use super::{ViewerState, ViewerStateError, compute_recipe, session::PublishOutcome};
use crate::{
    ports::{GitClient, ProjectComparisonReader, UserSettingsReader},
    recipes::{RecipeOp, RecipeTarget},
};

#[derive(Debug, thiserror::Error)]
pub enum SetModifiedFilesError {
    #[error(transparent)]
    State(#[from] ViewerStateError),
    #[error("viewer tab is unavailable or a commit selection is pending")]
    Unavailable,
    #[error("viewer comparison changed")]
    Changed,
    #[error(transparent)]
    Compute(#[from] compute_recipe::ComputeRecipeError),
}

#[cqrsy::command]
pub fn execute(
    request: SetViewerModifiedFiles,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
    comparisons: &impl ProjectComparisonReader,
) -> Result<(), SetModifiedFilesError> {
    if !request.visible {
        return state
            .update(|session| session.hide_modified_files(request.tab_id))?
            .then_some(())
            .ok_or(SetModifiedFilesError::Unavailable);
    }
    let (ticket, mut recipe) = state
        .update(|session| session.begin_modified_files(request.tab_id))?
        .ok_or(SetModifiedFilesError::Unavailable)?;
    recipe.op = RecipeOp::Diff {
        target: RecipeTarget::Base {
            rev: gtl_models::git::GitRevision::head(),
        },
    };
    let view = compute_recipe::execute(recipe, settings, git, comparisons)?;
    let snapshot = state.prepare_snapshot(Arc::new(view))?;
    match state.update(|session| session.publish_modified_files(ticket, snapshot))? {
        PublishOutcome::Published => Ok(()),
        PublishOutcome::Stale => Err(SetModifiedFilesError::Changed),
    }
}
