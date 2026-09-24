use gtl_models::{
    failure::{ErrorMeta, Failure, ViewerFailure},
    git::GitHeadState,
    viewer::{ViewerTabId, ViewerTabKind, ViewerTabState},
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

/// Why a live view could not be refreshed.
#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum RefreshLiveViewError {
    /// The live source cannot produce a view for a typed reason.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LiveViewState {
    head: GitHeadState,
    comparison: Option<crate::projects::comparison::ResolvedComparison>,
}

impl LiveViewState {
    pub(super) const fn is_branch_comparison(&self) -> bool {
        self.comparison.is_some()
    }
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
}

pub enum LiveViewCheck {
    Inactive,
    Unchanged,
    ChangedDuringComputation,
    Prepared(LiveViewPublication),
}

pub struct LiveViewPublication {
    ticket: ComputeTicket,
    head: LiveViewState,
    value: CachedView,
    label: String,
}

pub fn prepare(
    tab_id: ViewerTabId,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> Result<LiveViewCheck, RefreshLiveViewError> {
    let Some(request) = state.inspect(|session| session.live_refresh_request(tab_id))? else {
        return Ok(LiveViewCheck::Inactive);
    };
    let excluded = state.inspect(|session| session.file_exclusions(tab_id))?;
    let settings = super::settings::TabSettings::new(settings.clone(), excluded);
    let head = inspect_recipe(&request.recipe, git, comparisons)?;
    if request.head.as_ref() == Some(&head) {
        return Ok(LiveViewCheck::Unchanged);
    }
    let result = prepare_recipe::execute(
        PrepareRecipe {
            recipe: request.recipe.clone(),
            kind: ViewerTabKind::Live,
        },
        &settings,
        git,
        comparisons,
    )?;
    if inspect_recipe(&request.recipe, git, comparisons)? != head {
        return Ok(LiveViewCheck::ChangedDuringComputation);
    }
    match result {
        PrepareRecipeOk::Publish { label, view, .. } => {
            Ok(LiveViewCheck::Prepared(LiveViewPublication {
                ticket: request.ticket,
                head,
                value: CachedView::from_snapshot(state.prepare_snapshot(view)?),
                label,
            }))
        }
        PrepareRecipeOk::Broken { state } => Err(match state {
            ViewerTabState::Error { failure } => RefreshLiveViewError::Refused(failure),
            ViewerTabState::Broken { failure } => RefreshLiveViewError::Refused(failure.into()),
            ViewerTabState::Pending => {
                RefreshLiveViewError::Refused(ViewerFailure::SourcePreparing.into())
            }
            ViewerTabState::Ready => {
                anyhow::anyhow!("a broken live comparison reported a ready tab").into()
            }
        }),
        PrepareRecipeOk::Skipped { .. } => {
            Err(anyhow::anyhow!("live comparison produced no view").into())
        }
    }
}

pub fn publish(
    work: LiveViewPublication,
    state: &ViewerState,
) -> Result<PublishOutcome, super::ViewerStateError> {
    state.update(|session| {
        session.publish_live_if_current(work.ticket, work.head, work.value, work.label)
    })
}
