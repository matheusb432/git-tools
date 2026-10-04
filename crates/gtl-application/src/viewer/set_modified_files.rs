use std::sync::Arc;

use gtl_models::failure::{ErrorMeta, Failure, ViewerFailure};
use gtl_wire::viewer::SetViewerModifiedFiles;

use super::{ViewerState, ViewerStateError, compute_recipe, session::PublishOutcome};
use crate::{
    ports::{ExtensionFilterReader, GitClient, ProjectComparisonReader, UserSettingsReader},
    recipes::{RecipeOp, RecipeTarget},
};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum SetModifiedFilesError {
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error("viewer tab is unavailable or a commit selection is pending")]
    #[meta(failure = ViewerFailure::ModifiedFilesUnavailable)]
    Unavailable,
    #[error("viewer comparison changed")]
    #[meta(failure = Failure::Changed)]
    Changed,
    #[error(transparent)]
    #[meta(private(Internal))]
    Compute(#[from] compute_recipe::ComputeRecipeError),
}

#[cqrsy::command]
pub fn execute(
    request: SetViewerModifiedFiles,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
    filters: &impl ExtensionFilterReader,
    comparisons: &impl ProjectComparisonReader,
) -> Result<(), SetModifiedFilesError> {
    if !request.visible {
        return state
            .update(|session| session.hide_modified_files(request.tab_id))?
            .then_some(())
            .ok_or(SetModifiedFilesError::Unavailable);
    }
    let (ticket, root) = state
        .update(|session| session.begin_modified_files(request.tab_id))?
        .ok_or(SetModifiedFilesError::Unavailable)?;
    let tab_filter = state.inspect(|session| session.tab_extension_filter(request.tab_id))?;
    let filters = super::settings::TabExtensionFilters::new(filters, tab_filter);
    let view = compute_recipe::repository_view(
        root,
        RecipeOp::Diff {
            target: RecipeTarget::Base {
                rev: gtl_models::git::GitRevision::head(),
            },
        },
        None,
        settings,
        git,
        &filters,
        comparisons,
    )?;
    let snapshot = state.prepare_snapshot(Arc::new(view))?;
    match state.update(|session| session.publish_modified_files(ticket, snapshot))? {
        PublishOutcome::Published => Ok(()),
        PublishOutcome::Stale => Err(SetModifiedFilesError::Changed),
    }
}
