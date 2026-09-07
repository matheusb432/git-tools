use gtl_models::{
    git::GitHeadState,
    viewer::{ViewerTabId, ViewerTabKind, ViewerTabState},
};

use super::{
    ViewerState,
    prepare_recipe::{self, PrepareRecipe, PrepareRecipeOk},
    session::{CachedView, ComputeTicket, PublishOutcome},
};
use crate::{
    ports::{GitClient, UserSettingsReader},
    recipes::Recipe,
};

pub(super) struct LiveViewRefresh {
    pub(super) ticket: ComputeTicket,
    pub(super) recipe: Recipe,
    pub(super) head: Option<GitHeadState>,
}

pub enum LiveViewCheck {
    Inactive,
    Unchanged,
    ChangedDuringComputation,
    Prepared(LiveViewPublication),
}

pub struct LiveViewPublication {
    ticket: ComputeTicket,
    head: GitHeadState,
    value: CachedView,
    label: String,
}

pub fn prepare(
    tab_id: ViewerTabId,
    state: &ViewerState,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
) -> anyhow::Result<LiveViewCheck> {
    let Some(request) = state.inspect(|session| session.live_refresh_request(tab_id))? else {
        return Ok(LiveViewCheck::Inactive);
    };
    let path = request.recipe.cwd();
    let head = git.head_state(&path)?;
    if request.head.as_ref() == Some(&head) {
        return Ok(LiveViewCheck::Unchanged);
    }
    let result = prepare_recipe::execute(
        PrepareRecipe {
            recipe: request.recipe,
            kind: ViewerTabKind::Live,
        },
        settings,
        git,
    )?;
    if git.head_state(&path)? != head {
        return Ok(LiveViewCheck::ChangedDuringComputation);
    }
    match result {
        PrepareRecipeOk::Publish { label, view, .. } => {
            Ok(LiveViewCheck::Prepared(LiveViewPublication {
                ticket: request.ticket,
                head,
                value: CachedView::new(view),
                label,
            }))
        }
        PrepareRecipeOk::Broken { state } => match state {
            ViewerTabState::Error { reason } => anyhow::bail!(reason),
            state => anyhow::bail!("live comparison is unavailable: {state:?}"),
        },
        PrepareRecipeOk::Skipped { .. } => anyhow::bail!("live comparison produced no view"),
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
