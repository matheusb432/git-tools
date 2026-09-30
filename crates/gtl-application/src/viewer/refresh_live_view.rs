use gtl_models::{
    failure::{ErrorMeta, Failure, ViewerFailure},
    git::GitHeadState,
    viewer::{ViewerTabId, ViewerTabState},
};

use super::{
    ViewerState, ViewerStateError,
    prepare_recipe::{self, PrepareRecipe, PrepareRecipeError, PrepareRecipeOk},
    session::{CachedView, ComputeTicket, PublishOutcome},
};
use crate::{
    ports::{GitClient, UserSettingsReader},
    projects::comparison::ComparisonError,
    recipes::Recipe,
};

/// Why a live tab could not be refreshed.
#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum RefreshLiveViewError {
    /// The tab's source cannot produce a view for a typed reason.
    #[error(transparent)]
    #[meta(failure)]
    Refused(Failure),
    #[error(transparent)]
    #[meta(transparent)]
    Comparison(#[from] ComparisonError),
    #[error(transparent)]
    #[meta(transparent)]
    Prepare(#[from] PrepareRecipeError),
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

/// The source state a tab's content reflects; a live tab updates when it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LiveViewState {
    head: GitHeadState,
    comparison: Option<crate::projects::comparison::ResolvedComparison>,
}

pub(super) fn inspect_recipe(
    recipe: &Recipe,
    git: &impl GitClient,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<LiveViewState, RefreshLiveViewError> {
    let path = recipe.cwd();
    let head = git.head_state(&path)?;
    let comparison = if matches!(
        recipe.op,
        crate::recipes::RecipeOp::Diff {
            target: crate::recipes::RecipeTarget::Unpushed { pinned: None }
        }
    ) {
        match crate::projects::comparison::resolve(&path, git, comparisons)? {
            value @ crate::projects::comparison::ResolvedComparison::Branch { .. } => Some(value),
            crate::projects::comparison::ResolvedComparison::Upstream { .. } => None,
        }
    } else {
        None
    };
    Ok(LiveViewState { head, comparison })
}

#[cfg(test)]
impl From<GitHeadState> for LiveViewState {
    fn from(head: GitHeadState) -> Self {
        Self {
            head,
            comparison: None,
        }
    }
}

pub(super) struct LiveViewRefresh {
    pub(super) ticket: ComputeTicket,
    pub(super) recipe: Recipe,
    pub(super) head: Option<LiveViewState>,
    pub(super) changes_since: Option<gtl_models::timestamps::MachineTimestamp>,
}

pub enum LiveViewCheck {
    Inactive,
    Unchanged,
    ChangedDuringComputation,
    Prepared(Box<LiveViewPublication>),
}

pub struct LiveViewPublication {
    comparison_name: Option<gtl_models::git::GitRevision>,
    ticket: ComputeTicket,
    head: LiveViewState,
    value: CachedView,
    label: gtl_models::recipes::RecipeLabel,
    recipe: Recipe,
}

pub fn prepare(
    tab_id: ViewerTabId,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
    filters: &impl crate::ports::ExtensionFilterReader,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<LiveViewCheck, RefreshLiveViewError> {
    let Some(request) = state.inspect(|session| session.live_refresh_request(tab_id))? else {
        return Ok(LiveViewCheck::Inactive);
    };
    let tab_filter = state.inspect(|session| session.tab_extension_filter(tab_id))?;
    let filters = super::settings::TabExtensionFilters::new(filters, tab_filter);
    let head = inspect_recipe(&request.recipe, git, comparisons)?;
    if request.head.as_ref() == Some(&head) {
        return Ok(LiveViewCheck::Unchanged);
    }
    let result = prepare_recipe::execute(
        PrepareRecipe {
            comparison_name: None,
            recipe: request.recipe.clone(),
            changes_since: request.changes_since.clone(),
        },
        settings,
        git,
        &filters,
        comparisons,
    )?;
    if inspect_recipe(&request.recipe, git, comparisons)? != head {
        return Ok(LiveViewCheck::ChangedDuringComputation);
    }
    match result {
        PrepareRecipeOk::Publish {
            label,
            view,
            history,
        } => Ok(LiveViewCheck::Prepared(Box::new(LiveViewPublication {
            ticket: request.ticket,
            head,
            value: CachedView::from_snapshot(state.prepare_snapshot(view)?),
            label,
            recipe: history.recipe,
            comparison_name: history.comparison_name,
        }))),
        PrepareRecipeOk::Broken { state } => Err(match state {
            ViewerTabState::Error { failure } => RefreshLiveViewError::Refused(failure),
            ViewerTabState::Broken { failure } => RefreshLiveViewError::Refused(failure.into()),
            ViewerTabState::Pending => {
                RefreshLiveViewError::Refused(ViewerFailure::SourcePreparing.into())
            }
            ViewerTabState::Ready => {
                anyhow::anyhow!("a broken live tab source reported a ready tab").into()
            }
        }),
    }
}

pub fn publish(
    work: LiveViewPublication,
    state: &ViewerState,
) -> Result<PublishOutcome, super::ViewerStateError> {
    state.update(|session| {
        let outcome = session.publish_live_if_current(
            work.ticket,
            work.head,
            work.value,
            work.label,
            &work.recipe,
        );
        if outcome == PublishOutcome::Published {
            session.set_comparison_name(work.ticket.tab_id, work.comparison_name);
        }
        outcome
    })
}
