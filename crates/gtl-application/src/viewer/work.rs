//! Reserves viewer work, performs slow computation outside the session lock, and publishes results.

use std::sync::Arc;

use gtl_models::{
    diffs::CommitId,
    failure::{Classified as _, ErrorMeta, Failure, Resource, ViewerFailure},
    recipes::RecipeLabel,
    viewer::{ViewerTabId, ViewerTabState},
};

use super::{
    ViewerState, ViewerStateError,
    prepare_recipe::{self, PrepareRecipe, PrepareRecipeError, PrepareRecipeOk},
    session::{
        BeginCommitSelectionError, CachedView, CommitPatchTicket, ComputeTicket,
        EmptySnapshotOutcome, PublishOutcome, ViewerSession,
    },
};
use crate::{
    diffs::compute_commit_patch::{self, ComputeCommitPatch, ComputeCommitPatchError},
    history::{RecentRenderRecord, record_render::RecordRender},
    ports::{ExtensionFilterReader, GitClient, UserSettingsReader},
    recipes::{Recipe, RecipeBatch, RecipeBatchId, recipe_label},
};

#[derive(Debug)]
pub struct ReservedRecipeWork {
    pub history_id: Option<super::RenderHistoryId>,
    pub comparison_name: Option<gtl_models::git::GitRevision>,
    tab_filter: Option<gtl_models::diffs::ExtensionFilter>,
    /// The recipe to compute: pinned to reload the displayed commits, or unpinned to resolve
    /// the tab's revisions again.
    recipe: Recipe,
    ticket: ComputeTicket,
    /// Closes an unpinned tab whose opening computation finds nothing to show.
    skip_empty: bool,
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
    tab_filter: Option<gtl_models::diffs::ExtensionFilter>,
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
    skip_empty: bool,
}

/// Contains completed commit work ready for one publication attempt.
#[derive(Debug)]
pub struct ComputedCommitWork {
    ticket: CommitPatchTicket,
    result: Result<crate::diffs::View, ComputeCommitPatchError>,
}

#[derive(Debug)]
pub enum RecipePublication {
    Published { history: RecordRender },
    Broken { state: ViewerTabState },
    Skipped,
    Failed { error: PrepareRecipeError },
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

/// Opens `recipe` in a focused tab and reserves its computation; an empty result stays open.
pub fn reserve_open(
    state: &ViewerState,
    recipe: Recipe,
    batch_id: RecipeBatchId,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    reserve_opened(state, recipe, batch_id, false)
}

/// Opens every batch recipe; a recipe that finds nothing to show closes its unpinned tab.
pub fn reserve_recipe_batch(
    state: &ViewerState,
    batch: RecipeBatch,
) -> Result<Vec<ReservedRecipeWork>, ReserveRecipeError> {
    batch
        .recipes
        .into_iter()
        .map(|recipe| reserve_opened(state, recipe, batch.batch_id, true))
        .collect()
}

fn reserve_opened(
    state: &ViewerState,
    recipe: Recipe,
    batch_id: RecipeBatchId,
    skip_empty: bool,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    state.update(move |session| {
        let tab_id = session
            .open(recipe.clone(), batch_id)
            .ok_or(ReserveRecipeError::TabIdentifiersExhausted)?;
        let ticket = session
            .begin_compute(tab_id)
            .ok_or(ReserveRecipeError::UnknownTab)?;
        session.request_focus();
        Ok(ReservedRecipeWork {
            history_id: None,
            comparison_name: None,
            tab_filter: session.tab_extension_filter(ticket.tab_id),
            recipe,
            ticket,
            skip_empty,
        })
    })?
}

pub fn reserve_history_open(
    state: &ViewerState,
    record: RecentRenderRecord,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    let mut work = reserve_open(state, record.recipe.clone(), RecipeBatchId::generate())?;
    work.history_id = Some(record.id);
    work.comparison_name = record.comparison_name;
    state.update(|session| {
        session.restore_history_id(work.ticket.tab_id, work.history_id);
        session.set_comparison_name(work.ticket.tab_id, work.comparison_name.clone());
    })?;
    Ok(work)
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

/// Recomputes the tab's displayed commits, or a live tab's current source.
pub fn reserve_refresh(
    state: &ViewerState,
    tab_id: ViewerTabId,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    state.update(|session| reserve_refresh_in_session(session, tab_id))?
}

/// Recomputes the tab from its recipe's revisions as they resolve now.
pub fn reserve_update(
    state: &ViewerState,
    tab_id: ViewerTabId,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    state.update(|session| reserve_compute_in_session(session, tab_id, true))?
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
    let live = session.tab(tab_id).is_some_and(|tab| tab.tab.live());
    reserve_compute_in_session(session, tab_id, live)
}

pub(crate) fn reserve_restore_in_session(
    session: &mut ViewerSession,
    tab_id: ViewerTabId,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    let history_id = session
        .tab(tab_id)
        .ok_or(ReserveRecipeError::UnknownTab)?
        .history_id;
    let mut work = reserve_refresh_in_session(session, tab_id)?;
    work.history_id = history_id;
    session.restore_history_id(tab_id, history_id);
    Ok(work)
}

fn reserve_compute_in_session(
    session: &mut ViewerSession,
    tab_id: ViewerTabId,
    update: bool,
) -> Result<ReservedRecipeWork, ReserveRecipeError> {
    let tab = session.tab(tab_id).ok_or(ReserveRecipeError::UnknownTab)?;
    let history_id = (!update).then_some(tab.history_id).flatten();
    let comparison_name = if update {
        None
    } else {
        tab.comparison_name.clone()
    };
    let recipe = if update {
        tab.recipe.unpinned()
    } else {
        tab.recipe.clone()
    };
    let ticket = session
        .refresh(tab_id)
        .ok_or(ReserveRecipeError::UnknownTab)?;
    session.restore_history_id(tab_id, history_id);
    Ok(ReservedRecipeWork {
        history_id,
        comparison_name,
        tab_filter: session.tab_extension_filter(ticket.tab_id),
        recipe,
        ticket,
        skip_empty: false,
    })
}

pub fn compute_recipe(
    work: ReservedRecipeWork,
    settings: &impl UserSettingsReader,
    git: &impl GitClient,
    filters: &impl ExtensionFilterReader,
    comparisons: &impl crate::ports::ProjectComparisonReader,
) -> ComputedRecipeWork {
    let ReservedRecipeWork {
        history_id: _,
        comparison_name,
        tab_filter,
        recipe,
        ticket,
        skip_empty,
    } = work;
    let filters = super::settings::TabExtensionFilters::new(filters, tab_filter);
    // Only a computation that resolves the tab's revisions shows the source's current state.
    let head_before = (!recipe.is_pinned())
        .then(|| super::refresh_live_view::inspect_recipe(&recipe, git, comparisons).ok())
        .flatten();
    let result = prepare_recipe::execute(
        PrepareRecipe {
            comparison_name,
            recipe: recipe.clone(),
        },
        settings,
        git,
        &filters,
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
        skip_empty,
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
        skip_empty,
    } = work;
    match result {
        Ok(PrepareRecipeOk::Publish {
            label,
            view,
            history,
        }) => {
            let skipped_label = (skip_empty && !view.has_diff_content())
                .then(|| recipe_label::pending(&history.recipe));
            let value = CachedView::from_snapshot(state.prepare_snapshot(view)?);
            state.update(|session| {
                let outcome = skipped_label.map_or(EmptySnapshotOutcome::Kept, |skipped_label| {
                    session.skip_empty_snapshot_if_current(ticket, skipped_label)
                });
                match outcome {
                    EmptySnapshotOutcome::Skipped => RecipePublication::Skipped,
                    EmptySnapshotOutcome::Stale => RecipePublication::Stale,
                    EmptySnapshotOutcome::Kept => {
                        publish_view(session, ticket, value, label, *history, head)
                    }
                }
            })
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

fn publish_view(
    session: &mut ViewerSession,
    ticket: ComputeTicket,
    value: CachedView,
    label: RecipeLabel,
    history: RecordRender,
    head: Option<super::refresh_live_view::LiveViewState>,
) -> RecipePublication {
    match session.publish_labeled_if_current(ticket, value, label) {
        PublishOutcome::Published => {
            session.retain_snapshot_recipe(ticket, &history.recipe);
            session.set_comparison_name(ticket.tab_id, history.comparison_name.clone());
            session.set_live_head(ticket, head);
            RecipePublication::Published { history }
        }
        PublishOutcome::Stale => RecipePublication::Stale,
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
            tab_filter: session.tab_extension_filter(ticket.tab_id),
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
            tab_filter: session.tab_extension_filter(ticket.tab_id),
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
    filters: &impl ExtensionFilterReader,
) -> ComputedCommitWork {
    let ReservedCommitWork {
        tab_filter,
        repo_root,
        commit,
        ticket,
    } = work;
    let filters = super::settings::TabExtensionFilters::new(filters, tab_filter);
    let result = compute_commit_patch::execute(
        ComputeCommitPatch { repo_root, commit },
        settings,
        git,
        &filters,
    );
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
        viewer::get_viewer_shell,
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

    /// Reserves `path` as a one-recipe batch, which closes the tab when it finds nothing.
    fn reserve_pending(state: &ViewerState, path: &str) -> ReservedRecipeWork {
        reserve_recipe_batch(
            state,
            RecipeBatch {
                batch_id: RecipeBatchId::generate(),
                recipes: vec![recipe_at(path)],
            },
        )
        .unwrap()
        .remove(0)
    }

    #[test]
    fn failed_computation_publishes_a_safe_error_without_reusing_the_lock() {
        let state = ViewerState::new();
        let work = reserve_open(&state, recipe(), RecipeBatchId::generate()).unwrap();
        let work = compute_recipe(
            work,
            &FixedUserSettingsStore::default(),
            &crate::utils::FakeGitClient {
                repository_state: Some(crate::ports::GitRepositoryState::Repository {
                    top_level: repository_root("/repo"),
                }),
                ..Default::default()
            },
            &crate::utils::SavedExtensionFilters::default(),
            &crate::utils::ProjectComparisons::default(),
        );

        let publication = publish_recipe(&state, work).unwrap();

        assert!(matches!(publication, RecipePublication::Failed { .. }));
        assert_eq!(
            state.version().unwrap(),
            gtl_models::viewer::ViewerVersion::new(4)
        );
    }

    /// Completes `work` with an empty view, as a render of a range without changes would.
    fn computed_empty(work: &ReservedRecipeWork) -> ComputedRecipeWork {
        ComputedRecipeWork {
            ticket: work.ticket(),
            result: Ok(PrepareRecipeOk::Publish {
                label: crate::utils::viewer::label("empty"),
                view: Arc::new(crate::utils::viewer::empty_view()),
                history: Box::new(RecordRender {
                    comparison_name: None,
                    recipe: work.recipe().clone(),
                    repo_name: crate::utils::project_name("empty"),
                    range_label: "main..HEAD".into(),
                    label_parts: crate::recipes::RecipeLabelParts::None,
                }),
            }),
            head: None,
            skip_empty: work.skip_empty,
        }
    }

    #[test]
    fn pinned_empty_snapshot_shows_its_empty_view() {
        let state = ViewerState::new();
        let reserved = reserve_pending(&state, "/repo/empty");
        let tab_id = reserved.ticket().tab_id;
        state
            .update(|session| session.set_pinned(tab_id, true))
            .unwrap();

        let publication = publish_recipe(&state, computed_empty(&reserved)).unwrap();

        assert!(matches!(publication, RecipePublication::Published { .. }));
        state
            .inspect(|session| {
                assert_eq!(
                    session.tab(tab_id).unwrap().tab.state(),
                    &gtl_models::viewer::ViewerTabState::Ready
                );
            })
            .unwrap();
    }

    #[test]
    fn an_explicitly_opened_empty_tab_shows_its_empty_view() {
        let state = ViewerState::new();
        let reserved =
            reserve_open(&state, recipe_at("/repo/empty"), RecipeBatchId::generate()).unwrap();

        let publication = publish_recipe(&state, computed_empty(&reserved)).unwrap();

        assert!(matches!(publication, RecipePublication::Published { .. }));
    }

    #[test]
    fn refresh_reloads_the_displayed_commits_while_update_and_live_tabs_resolve_them_again() {
        let state = ViewerState::new();
        let pinned = Recipe {
            op: RecipeOp::MergeDiff {
                base: Some(git_revision("main")),
                pinned: Some(crate::utils::pinned_range("a", "b")),
            },
            ..recipe()
        };
        let tab_id = reserve_open(&state, pinned.clone(), RecipeBatchId::generate())
            .unwrap()
            .ticket()
            .tab_id;

        assert_eq!(reserve_refresh(&state, tab_id).unwrap().recipe(), &pinned);
        assert_eq!(reserve_update(&state, tab_id).unwrap().recipe(), &recipe());
        state
            .update(|session| session.set_live(tab_id, true))
            .unwrap();
        assert_eq!(reserve_refresh(&state, tab_id).unwrap().recipe(), &recipe());
    }

    #[test]
    fn skipped_snapshot_reaches_the_next_shell_once() {
        let state = ViewerState::new();
        let reserved = reserve_pending(&state, "/repo/empty");
        let label = crate::recipes::recipe_label::pending(reserved.recipe());
        let work = computed_empty(&reserved);
        let shell_feedback = || {
            get_viewer_shell::execute(&state, &FixedUserSettingsStore::default())
                .unwrap()
                .shell
                .feedback
        };

        let publication = publish_recipe(&state, work).unwrap();

        assert!(matches!(publication, RecipePublication::Skipped));
        assert_eq!(
            shell_feedback(),
            Some(gtl_wire::viewer::ViewerFeedback::SnapshotRecipesSkipped {
                labels: vec![label],
            })
        );
        assert_eq!(shell_feedback(), None);
    }

    #[test]
    fn activating_a_pending_tab_reserves_its_refresh() {
        let state = ViewerState::new();
        let initial = reserve_pending(&state, "/repo");

        let refresh = activate_tab(&state, initial.ticket().tab_id)
            .unwrap()
            .unwrap();

        assert_eq!(refresh.ticket().tab_id, initial.ticket().tab_id);
        assert_ne!(refresh.ticket().generation, initial.ticket().generation);
    }

    #[test]
    fn reopening_the_same_tab_requests_focus_but_refreshing_does_not() {
        let state = ViewerState::new();
        let first = reserve_pending(&state, "/repo");
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

        let reopened = reserve_pending(&state, "/repo");
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
