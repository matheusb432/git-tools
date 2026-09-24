//! Reserves viewer work, performs slow computation outside the session lock, and publishes results.

use std::sync::Arc;

use gtl_models::{
    diffs::CommitId,
    failure::{Classified as _, ErrorMeta, Failure, Resource, ViewerFailure},
    viewer::{ViewerTabId, ViewerTabKind, ViewerTabState},
};

use super::{
    ViewerState, ViewerStateError,
    prepare_recipe::{self, PrepareRecipe, PrepareRecipeError, PrepareRecipeOk},
    session::{
        BeginCommitSelectionError, CachedView, CommitPatchTicket, ComputeTicket, PublishOutcome,
        ViewerSession,
    },
};
use crate::{
    diffs::compute_commit_patch::{self, ComputeCommitPatch, ComputeCommitPatchError},
    history::{RecentRenderRecord, record_render::RecordRender},
    live_views::{LiveViewRecord, recipe_for_record},
    ports::{GitClient, UserSettingsReader},
    recipes::{Recipe, RecipeBatch, RecipeBatchId, RecipeBatchKind},
};

#[derive(Debug)]
pub struct ReservedRecipeWork {
    excluded: Option<gtl_models::diffs::ExcludedExtensions>,
    recipe: Recipe,
    kind: ViewerTabKind,
    ticket: ComputeTicket,
}

impl ReservedRecipeWork {
    #[must_use]
    pub const fn ticket(&self) -> ComputeTicket {
        self.ticket
    }

    #[must_use]
    pub const fn recipe(&self) -> &Recipe {
        &self.recipe
    }
}

#[derive(Debug)]
pub struct ReservedCommitWork {
    excluded: Option<gtl_models::diffs::ExcludedExtensions>,
    repo_root: gtl_models::paths::RepositoryRoot,
    commit: gtl_models::diffs::Commit,
    ticket: CommitPatchTicket,
}

impl ReservedCommitWork {
    #[must_use]
    pub const fn ticket(&self) -> CommitPatchTicket {
        self.ticket
    }
}

/// Contains completed recipe work ready for one publication attempt.
#[derive(Debug)]
pub struct ComputedRecipeWork {
    ticket: ComputeTicket,
    result: Result<PrepareRecipeOk, PrepareRecipeError>,
    head: Option<super::refresh_live_view::LiveViewState>,
}

/// Contains completed commit work ready for one publication attempt.
#[derive(Debug)]
pub struct ComputedCommitWork {
    ticket: CommitPatchTicket,
    result: Result<crate::diffs::View, ComputeCommitPatchError>,
}

#[derive(Debug)]
pub enum RecipePublication {
    Published {
        history: RecordRender,
    },
    Broken {
        state: ViewerTabState,
    },
    Skipped {
        path: gtl_models::paths::RepositoryRoot,
    },
    Failed {
        error: PrepareRecipeError,
    },
    Stale,
}

#[derive(Debug)]
pub enum CommitPublication {
    Published,
    Failed { error: ComputeCommitPatchError },
    Stale,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ReserveRecipeError {
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error("viewer tab identifiers are exhausted")]
    #[meta(private(Internal))]
    TabIdentifiersExhausted,
    #[error("viewer tab is not available")]
    #[meta(failure = Failure::Gone { resource: Resource::ViewerTab })]
    UnknownTab,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ReserveCommitError {
    #[error(transparent)]
    #[meta(transparent)]
    State(#[from] ViewerStateError),
    #[error(transparent)]
    #[meta(transparent)]
    Selection(#[from] BeginCommitSelectionError),
}

pub fn reserve_open(
    state: &ViewerState,
    recipe: Recipe,
    batch_id: RecipeBatchId,
    kind: ViewerTabKind,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    state.update(move |session| {
        let tab_id = session
            .open(recipe.clone(), batch_id, kind)
            .ok_or(ReserveRecipeError::TabIdentifiersExhausted)?;
        let ticket = session
            .begin_compute(tab_id)
            .ok_or(ReserveRecipeError::UnknownTab)?;
        session.request_focus();
        Ok(ReservedRecipeWork {
            excluded: session.file_exclusions(ticket.tab_id),
            recipe,
            kind,
            ticket,
        })
    })?
}

pub fn reserve_recipe_batch(
    state: &ViewerState,
    batch: RecipeBatch,
) -> Result<Vec<ReservedRecipeWork>, ReserveRecipeError> {
    let kind = match batch.kind {
        RecipeBatchKind::Snapshot => ViewerTabKind::Snapshot,
        RecipeBatchKind::Live => ViewerTabKind::Live,
    };
    batch
        .recipes
        .into_iter()
        .map(|recipe| reserve_open(state, recipe, batch.batch_id, kind))
        .collect()
}

pub(crate) fn reserve_restored_pin(
    state: &ViewerState,
    recipe: Recipe,
    kind: ViewerTabKind,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    state.update(|session| {
        let id = session
            .open(recipe, RecipeBatchId::generate(), kind)
            .ok_or(ReserveRecipeError::TabIdentifiersExhausted)?;
        session.set_pinned(id, true);
        reserve_refresh_in_session(session, id)
    })?
}

pub fn reserve_restored_live_views(
    state: &ViewerState,
    records: impl IntoIterator<Item = LiveViewRecord>,
) -> Result<Option<ReservedRecipeWork>, ReserveRecipeError> {
    state.update(move |session| {
        let mut newest = None;
        for record in records {
            newest = Some(
                session
                    .open(
                        recipe_for_record(&record),
                        RecipeBatchId::generate(),
                        ViewerTabKind::Live,
                    )
                    .ok_or(ReserveRecipeError::TabIdentifiersExhausted)?,
            );
        }
        newest
            .map(|tab_id| reserve_refresh_in_session(session, tab_id))
            .transpose()
    })?
}

pub fn reserve_history_open(
    state: &ViewerState,
    record: RecentRenderRecord,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    reserve_open(
        state,
        record.recipe,
        RecipeBatchId::generate(),
        ViewerTabKind::Snapshot,
    )
}

pub fn activate_tab(
    state: &ViewerState,
    tab_id: ViewerTabId,
) -> Result<Option<ReservedRecipeWork>, ReserveRecipeError> {
    state.update(|session| {
        if !session.activate(tab_id) {
            return Err(ReserveRecipeError::UnknownTab);
        }
        reserve_active_if_needed(session)
    })?
}

pub fn reserve_refresh(
    state: &ViewerState,
    tab_id: ViewerTabId,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    state.update(|session| reserve_refresh_in_session(session, tab_id))?
}

pub(crate) fn reserve_active_if_needed(
    session: &mut ViewerSession,
) -> Result<Option<ReservedRecipeWork>, ReserveRecipeError> {
    let Some(tab_id) = session.active().filter(|_| active_needs_refresh(session)) else {
        return Ok(None);
    };
    reserve_refresh_in_session(session, tab_id).map(Some)
}

fn active_needs_refresh(session: &mut ViewerSession) -> bool {
    let Some(active) = session.active() else {
        return false;
    };
    let pending = session
        .tab(active)
        .is_some_and(|tab| matches!(tab.tab.state(), ViewerTabState::Pending));
    let ready = session
        .tab(active)
        .is_some_and(|tab| matches!(tab.tab.state(), ViewerTabState::Ready));
    pending || (ready && session.cached_view_snapshot(active).is_none())
}

fn reserve_refresh_in_session(
    session: &mut ViewerSession,
    tab_id: ViewerTabId,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    let tab = session.tab(tab_id).ok_or(ReserveRecipeError::UnknownTab)?;
    let recipe = tab.recipe.clone();
    let kind = tab.tab.kind();
    let ticket = session
        .refresh(tab_id)
        .ok_or(ReserveRecipeError::UnknownTab)?;
    Ok(ReservedRecipeWork {
        excluded: session.file_exclusions(ticket.tab_id),
        recipe,
        kind,
        ticket,
    })
}

pub fn compute_recipe(
    work: ReservedRecipeWork,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> ComputedRecipeWork {
    let ReservedRecipeWork {
        excluded,
        recipe,
        kind,
        ticket,
    } = work;
    let settings = super::settings::TabSettings::new(settings.clone(), excluded);
    let head_before = (kind == ViewerTabKind::Live)
        .then(|| super::refresh_live_view::inspect_recipe(&recipe, git, comparisons).ok())
        .flatten();
    let result = prepare_recipe::execute(
        PrepareRecipe {
            recipe: recipe.clone(),
            kind,
        },
        &settings,
        git,
        comparisons,
    );
    let head = head_before.filter(|before| {
        super::refresh_live_view::inspect_recipe(&recipe, git, comparisons)
            .as_ref()
            .ok()
            == Some(before)
    });
    ComputedRecipeWork {
        ticket,
        result,
        head,
    }
}

/// Consumes completed work so one computation cannot be published twice.
pub fn publish_recipe(
    state: &ViewerState,
    work: ComputedRecipeWork,
) -> Result<RecipePublication, ViewerStateError> {
    let ComputedRecipeWork {
        ticket,
        result,
        head,
    } = work;
    match result {
        Ok(PrepareRecipeOk::Publish {
            label,
            view,
            history,
        }) => {
            let value = CachedView::from_snapshot(state.prepare_snapshot(view)?);
            state.update(
                |session| match session.publish_labeled_if_current(ticket, value, label) {
                    PublishOutcome::Published => {
                        session.retain_snapshot_recipe(ticket, &history.recipe);
                        session.set_live_head(ticket, head);
                        RecipePublication::Published { history }
                    }
                    PublishOutcome::Stale => RecipePublication::Stale,
                },
            )
        }
        Ok(PrepareRecipeOk::Broken { state: broken }) => {
            let published = broken.clone();
            state.update(
                |session| match session.set_state_if_current(ticket, published) {
                    PublishOutcome::Published => RecipePublication::Broken { state: broken },
                    PublishOutcome::Stale => RecipePublication::Stale,
                },
            )
        }
        Ok(PrepareRecipeOk::Skipped { path, .. }) => {
            state.update(|session| match session.close_if_current(ticket) {
                PublishOutcome::Published => RecipePublication::Skipped { path },
                PublishOutcome::Stale => RecipePublication::Stale,
            })
        }
        Err(error) => state.update(|session| {
            let outcome = session.set_state_if_current(
                ticket,
                gtl_models::viewer::ViewerTabState::Error {
                    failure: match &error {
                        PrepareRecipeError::Compute(
                            super::compute_recipe::ComputeRecipeError::Diff(
                                crate::diffs::compute_diff::ComputeDiffError::Comparison(error),
                            ),
                        ) if error.is_unavailable() => error.classify().into_failure(),
                        _ => ViewerFailure::RenderFailed.into(),
                    },
                },
            );
            match outcome {
                PublishOutcome::Published => RecipePublication::Failed { error },
                PublishOutcome::Stale => RecipePublication::Stale,
            }
        }),
    }
}

pub fn reserve_selected_commit_reload(
    state: &ViewerState,
) -> Result<Option<ReservedCommitWork>, ReserveCommitError> {
    state.update(|session| {
        let Some((tab_id, commit_id)) = session.selected_commit_to_reload() else {
            return Ok(None);
        };
        let (ticket, repo_root, commit) = session.begin_commit_selection(tab_id, &commit_id)?;
        Ok(Some(ReservedCommitWork {
            excluded: session.file_exclusions(ticket.tab_id),
            repo_root,
            commit,
            ticket,
        }))
    })?
}

pub fn reserve_commit(
    state: &ViewerState,
    tab_id: ViewerTabId,
    commit_id: &CommitId,
) -> Result<ReservedCommitWork, ReserveCommitError> {
    state.update(|session| {
        let (ticket, repo_root, commit) = session.begin_commit_selection(tab_id, commit_id)?;
        Ok(ReservedCommitWork {
            excluded: session.file_exclusions(ticket.tab_id),
            repo_root,
            commit,
            ticket,
        })
    })?
}

pub fn compute_commit(
    work: ReservedCommitWork,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
) -> ComputedCommitWork {
    let ReservedCommitWork {
        excluded,
        repo_root,
        commit,
        ticket,
    } = work;
    let settings = super::settings::TabSettings::new(settings.clone(), excluded);
    let result =
        compute_commit_patch::execute(ComputeCommitPatch { repo_root, commit }, &settings, git);
    ComputedCommitWork { ticket, result }
}

/// Consumes completed work so one commit computation cannot be published twice.
pub fn publish_commit(
    state: &ViewerState,
    work: ComputedCommitWork,
) -> Result<CommitPublication, ViewerStateError> {
    let ComputedCommitWork { ticket, result } = work;
    let result = match result {
        Ok(view) => Ok(state.prepare_snapshot(Arc::new(view))?),
        Err(error) => Err(error),
    };
    state.update(|session| match result {
        Ok(view) => match session.publish_commit_patch_if_current(ticket, view) {
            PublishOutcome::Published => CommitPublication::Published,
            PublishOutcome::Stale => CommitPublication::Stale,
        },
        Err(error) => match session
            .set_commit_patch_error_if_current(ticket, ViewerFailure::CommitFailed.into())
        {
            PublishOutcome::Published => CommitPublication::Failed { error },
            PublishOutcome::Stale => CommitPublication::Stale,
        },
    })
}

pub fn clear_commit_selection(
    state: &ViewerState,
    tab_id: ViewerTabId,
) -> Result<bool, ViewerStateError> {
    state.update(|session| session.clear_commit_selection(tab_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        recipes::{RecipeOp, RecipeSource},
        utils::{FixedUserSettingsStore, git_revision, repository_root},
    };

    fn recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(repository_root("/repo")),
            op: RecipeOp::MergeDiff {
                base: Some(git_revision("main")),
                pinned: None,
            },
            name: None,
        }
    }

    fn recipe_at(path: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(repository_root(path)),
            ..recipe()
        }
    }

    fn reserve_pending(state: &ViewerState, path: &str, kind: ViewerTabKind) -> ReservedRecipeWork {
        reserve_open(state, recipe_at(path), RecipeBatchId::generate(), kind).unwrap()
    }

    #[test]
    fn failed_computation_publishes_a_safe_error_without_reusing_the_lock() {
        let state = ViewerState::new();
        let work = reserve_open(
            &state,
            recipe(),
            RecipeBatchId::generate(),
            ViewerTabKind::Snapshot,
        )
        .unwrap();
        let work = compute_recipe(
            work,
            &FixedUserSettingsStore::default(),
            &crate::utils::FakeGitClient::default(),
            &crate::utils::ProjectComparisons::default(),
        );

        let publication = publish_recipe(&state, work).unwrap();

        assert!(matches!(publication, RecipePublication::Failed { .. }));
        assert_eq!(
            state.version().unwrap(),
            gtl_models::viewer::ViewerVersion::new(4)
        );
    }

    #[test]
    fn activating_a_pending_tab_reserves_its_refresh() {
        let state = ViewerState::new();
        let initial = reserve_pending(&state, "/repo", ViewerTabKind::Snapshot);

        let refresh = activate_tab(&state, initial.ticket().tab_id)
            .unwrap()
            .unwrap();

        assert_eq!(refresh.ticket().tab_id, initial.ticket().tab_id);
        assert_ne!(refresh.ticket().generation, initial.ticket().generation);
    }

    #[test]
    fn reopening_the_same_tab_requests_focus_but_refreshing_does_not() {
        let state = ViewerState::new();
        let first = reserve_pending(&state, "/repo", ViewerTabKind::Snapshot);
        let focus_first = state
            .inspect(|session| session.focus_request_version())
            .unwrap();
        assert!(focus_first.is_some());

        reserve_refresh(&state, first.ticket().tab_id).unwrap();
        state.mark_shell_changed().unwrap();
        assert_eq!(
            state
                .inspect(|session| session.focus_request_version())
                .unwrap(),
            focus_first
        );

        let reopened = reserve_pending(&state, "/repo", ViewerTabKind::Snapshot);
        let focus_reopened = state
            .inspect(|session| session.focus_request_version())
            .unwrap();
        assert_eq!(reopened.ticket().tab_id, first.ticket().tab_id);
        assert!(focus_reopened > focus_first);

        activate_tab(&state, first.ticket().tab_id).unwrap();
        assert_eq!(
            state
                .inspect(|session| session.focus_request_version())
                .unwrap(),
            focus_reopened
        );
    }
}
