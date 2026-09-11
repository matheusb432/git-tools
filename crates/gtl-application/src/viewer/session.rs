mod cache;

use std::sync::Arc;

pub use cache::{CacheDisposition, CachedView, ViewCacheWeight, WeightedViewCache};
use gtl_models::{
    diffs::{Commit, CommitId},
    live_views::LiveSource,
    recipes::RecipeBatchId,
    viewer::{
        ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTab, ViewerTabId, ViewerTabKind,
        ViewerTabPlacement, ViewerTabState, ViewerVersion,
    },
};

use crate::{
    diffs::View,
    recipes::{Recipe, RecipeSource},
    viewer::{ViewerDiffSnapshot, initial_recipe_label},
};

pub const DEFAULT_VIEW_CACHE_WEIGHT: ViewCacheWeight = ViewCacheWeight::new(128 * 1024 * 1024);

/// Marks a tab whose view is still being computed.
pub const RENDER_PENDING_REASON: &str = "render pending";

/// A generation token authorizing publication for one still-current compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputeTicket {
    pub tab_id: ViewerTabId,
    pub generation: ViewerRangeGeneration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitPatchTicket {
    pub tab_id: ViewerTabId,
    pub range_generation: ViewerRangeGeneration,
    pub selection_generation: ViewerSelectionGeneration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveContentIdentity {
    tab_id: ViewerTabId,
    range_generation: ViewerRangeGeneration,
    selection_generation: ViewerSelectionGeneration,
}

impl ActiveContentIdentity {
    #[must_use]
    pub const fn tab_id(self) -> ViewerTabId {
        self.tab_id
    }

    #[must_use]
    pub const fn range_generation(self) -> ViewerRangeGeneration {
        self.range_generation
    }

    #[must_use]
    pub const fn selection_generation(self) -> ViewerSelectionGeneration {
        self.selection_generation
    }
}

#[derive(Debug, Clone)]
pub struct ActiveContentSnapshot {
    identity: ActiveContentIdentity,
    view: Arc<View>,
}

impl ActiveContentSnapshot {
    #[must_use]
    pub const fn identity(&self) -> ActiveContentIdentity {
        self.identity
    }

    #[must_use]
    pub fn view(&self) -> &View {
        &self.view
    }

    #[must_use]
    pub fn shared_view(&self) -> Arc<View> {
        Arc::clone(&self.view)
    }
}

#[derive(Debug, Clone)]
enum CommitSelection {
    None,
    Pending {
        commit: Commit,
    },
    Ready {
        commit: Commit,
        transient: Option<ViewerDiffSnapshot>,
    },
    Error {
        commit: Commit,
        reason: String,
    },
}

fn selected_view(
    cache: &mut WeightedViewCache,
    id: ViewerTabId,
    transient: Option<&ViewerDiffSnapshot>,
) -> Option<ViewerDiffSnapshot> {
    transient
        .cloned()
        .or_else(|| cache.get(id).and_then(|cached| cached.selected.clone()))
}

#[derive(Debug, Clone)]
pub enum CommitSelectionSnapshot {
    None,
    Pending {
        id: CommitId,
    },
    Ready {
        id: CommitId,
        view: ViewerDiffSnapshot,
    },
    Error {
        id: CommitId,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BeginCommitSelectionError {
    #[error("viewer tab is not available")]
    UnknownTab,
    #[error("viewer range changed")]
    StaleRange,
    #[error("commit is not available in this range")]
    UnknownCommit,
    #[error("another commit selection is pending")]
    SelectionPending,
}

/// Whether a compute result was current enough to mutate the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Published,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    ActiveChanged,
    ActiveUnchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MoveOutcome {
    Moved,
    Unchanged,
}

/// Session-owned metadata for one recipe tab.
#[derive(Debug, Clone)]
pub struct SessionTab {
    pub tab: ViewerTab,
    pub recipe: Recipe,
    pub batch_id: RecipeBatchId,
    generation: ViewerRangeGeneration,
    selection_generation: ViewerSelectionGeneration,
    selection: CommitSelection,
    live_head: Option<super::refresh_live_view::LiveViewState>,
}

#[derive(Clone, Copy)]
enum FullContextPreparation {
    Pending,
    Failed,
}

/// Authoritative recipe tabs plus their separately bounded computed views.
pub struct ViewerSession {
    tabs: Vec<SessionTab>,
    cache: WeightedViewCache,
    active: Option<ViewerTabId>,
    next_id: Option<u64>,
    version: ViewerVersion,
    focus_request_version: Option<ViewerVersion>,
    full_context_transient: Option<(ActiveContentIdentity, ViewerDiffSnapshot)>,
    full_context_preparation: Option<(ActiveContentIdentity, FullContextPreparation)>,
}

impl ViewerSession {
    #[must_use]
    pub fn new(max_cache_weight: ViewCacheWeight) -> Self {
        Self {
            tabs: Vec::new(),
            cache: WeightedViewCache::new(max_cache_weight),
            active: None,
            next_id: Some(1),
            version: ViewerVersion::default(),
            focus_request_version: None,
            full_context_transient: None,
            full_context_preparation: None,
        }
    }

    pub fn open(
        &mut self,
        recipe: Recipe,
        batch_id: RecipeBatchId,
        kind: ViewerTabKind,
    ) -> Option<ViewerTabId> {
        let label = if kind == ViewerTabKind::Live {
            super::recipe_label::live(&recipe)
                .unwrap_or_else(|| initial_recipe_label::execute(&recipe))
        } else {
            initial_recipe_label::execute(&recipe)
        };
        self.open_labeled(recipe, batch_id, kind, label)
    }

    fn open_labeled(
        &mut self,
        recipe: Recipe,
        batch_id: RecipeBatchId,
        kind: ViewerTabKind,
        label: String,
    ) -> Option<ViewerTabId> {
        let unpinned = recipe.unpinned();
        if let Some(existing) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.recipe.unpinned() == unpinned)
        {
            existing.tab = ViewerTab::new(
                existing.tab.id(),
                existing.tab.label().into(),
                kind,
                existing.tab.state().clone(),
            );
            // Intentionally symmetric: reopening an unpinned (legacy/history) recipe
            // replaces a pinned one too, since the tab shows what was most recently opened.
            existing.recipe = recipe;
            existing.batch_id = batch_id;
            self.active = Some(existing.tab.id());
            let id = existing.tab.id();
            self.bump_version();
            return Some(id);
        }

        let id = self
            .next_id
            .and_then(|value| ViewerTabId::try_new(value).ok())?;
        self.next_id = self.next_id.and_then(|value| value.checked_add(1));
        self.tabs.push(SessionTab {
            tab: ViewerTab::new(
                id,
                label,
                kind,
                ViewerTabState::Error {
                    reason: RENDER_PENDING_REASON.into(),
                },
            ),
            recipe,
            batch_id,
            generation: ViewerRangeGeneration::default(),
            selection_generation: ViewerSelectionGeneration::default(),
            selection: CommitSelection::None,
            live_head: None,
        });
        self.active = Some(id);
        self.bump_version();
        Some(id)
    }

    pub fn begin_compute(&mut self, id: ViewerTabId) -> Option<ComputeTicket> {
        let tab = self.tabs.iter_mut().find(|tab| tab.tab.id() == id)?;
        self.cache.remove(id);
        tab.live_head = None;
        // Tickets are process-local and short-lived; wrapping would require 2^64 mutations while
        // one ticket remains in flight before an old ticket could compare equal again.
        tab.generation = tab.generation.next();
        tab.selection_generation = tab.selection_generation.next();
        tab.selection = match std::mem::replace(&mut tab.selection, CommitSelection::None) {
            CommitSelection::None => CommitSelection::None,
            CommitSelection::Pending { commit }
            | CommitSelection::Ready { commit, .. }
            | CommitSelection::Error { commit, .. } => CommitSelection::Ready {
                commit,
                transient: None,
            },
        };
        tab.tab = ViewerTab::new(
            id,
            tab.tab.label().into(),
            tab.tab.kind(),
            ViewerTabState::Error {
                reason: RENDER_PENDING_REASON.into(),
            },
        );
        let generation = tab.generation;
        self.bump_version();
        Some(ComputeTicket {
            tab_id: id,
            generation,
        })
    }

    pub(super) fn publish_labeled_if_current(
        &mut self,
        ticket: ComputeTicket,
        value: CachedView,
        label: String,
    ) -> PublishOutcome {
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == ticket.tab_id)
        else {
            return PublishOutcome::Stale;
        };
        if tab.generation != ticket.generation {
            return PublishOutcome::Stale;
        }

        if let CommitSelection::Ready { commit, .. } = &tab.selection
            && !value.view.commits.iter().any(|entry| entry.id == commit.id)
        {
            tab.selection = CommitSelection::None;
        }
        tab.tab = ViewerTab::new(ticket.tab_id, label, tab.tab.kind(), ViewerTabState::Ready);
        self.cache.insert(ticket.tab_id, value);
        self.bump_version();
        PublishOutcome::Published
    }

    pub(super) fn set_state_if_current(
        &mut self,
        ticket: ComputeTicket,
        state: ViewerTabState,
    ) -> PublishOutcome {
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == ticket.tab_id)
        else {
            return PublishOutcome::Stale;
        };
        if tab.generation != ticket.generation {
            return PublishOutcome::Stale;
        }
        let label = tab.tab.label().into();
        tab.tab = ViewerTab::new(ticket.tab_id, label, tab.tab.kind(), state);
        self.bump_version();
        PublishOutcome::Published
    }

    pub(super) fn live_refresh_request(
        &self,
        id: ViewerTabId,
    ) -> Option<super::refresh_live_view::LiveViewRefresh> {
        let tab = self.tab(id)?;
        if self.active != Some(id)
            || tab.tab.kind() != ViewerTabKind::Live
            || matches!(tab.tab.state(), ViewerTabState::Error { reason } if reason == RENDER_PENDING_REASON)
        {
            return None;
        }
        Some(super::refresh_live_view::LiveViewRefresh {
            ticket: ComputeTicket {
                tab_id: id,
                generation: tab.generation,
            },
            recipe: tab.recipe.clone(),
            head: tab.live_head.clone(),
        })
    }

    pub(super) fn is_branch_comparison(&self, tab_id: ViewerTabId) -> bool {
        self.tab(tab_id)
            .and_then(|tab| tab.live_head.as_ref())
            .is_some_and(super::refresh_live_view::LiveViewState::is_branch_comparison)
    }

    pub(super) fn retain_snapshot_recipe(&mut self, ticket: ComputeTicket, recipe: &Recipe) {
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == ticket.tab_id)
            && tab.tab.kind() == ViewerTabKind::Snapshot
        {
            tab.recipe = recipe.clone();
        }
    }

    pub(super) fn set_live_head(
        &mut self,
        ticket: ComputeTicket,
        head: Option<super::refresh_live_view::LiveViewState>,
    ) {
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == ticket.tab_id && tab.generation == ticket.generation)
        {
            tab.live_head = head;
        }
    }

    pub(super) fn publish_live_if_current(
        &mut self,
        ticket: ComputeTicket,
        head: super::refresh_live_view::LiveViewState,
        mut value: CachedView,
        label: String,
    ) -> PublishOutcome {
        if self.active != Some(ticket.tab_id) || self.current_ticket(ticket.tab_id) != Some(ticket)
        {
            return PublishOutcome::Stale;
        }
        let selection = self.commit_selection_snapshot(ticket.tab_id);
        if matches!(selection, CommitSelectionSnapshot::Pending { .. }) {
            return PublishOutcome::Stale;
        }
        let preserved = match selection {
            CommitSelectionSnapshot::Ready { id, view } => value
                .view
                .commits
                .iter()
                .find(|commit| commit.id == id)
                .cloned()
                .map(|commit| (commit, view)),
            _ => None,
        };
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == ticket.tab_id)
        else {
            return PublishOutcome::Stale;
        };
        tab.generation = tab.generation.next();
        tab.selection_generation = tab.selection_generation.next();
        tab.live_head = Some(head);
        tab.selection = if let Some((commit, selected)) = preserved {
            value = value.with_selected(selected.clone());
            CommitSelection::Ready {
                commit,
                transient: Some(selected),
            }
        } else {
            CommitSelection::None
        };
        tab.tab = ViewerTab::new(
            ticket.tab_id,
            label,
            ViewerTabKind::Live,
            ViewerTabState::Ready,
        );
        self.cache.insert(ticket.tab_id, value);
        self.bump_version();
        PublishOutcome::Published
    }

    pub fn refresh(&mut self, id: ViewerTabId) -> Option<ComputeTicket> {
        self.begin_compute(id)
    }

    pub(super) fn selected_commit_to_reload(&mut self) -> Option<(ViewerTabId, CommitId)> {
        let id = self.active?;
        let tab = self.tab(id)?;
        if !matches!(tab.tab.state(), ViewerTabState::Ready) {
            return None;
        }
        let CommitSelection::Ready { commit, transient } = &tab.selection else {
            return None;
        };
        if transient.is_some() {
            return None;
        }
        let commit = commit.id.clone();
        let cached = self.cache.get(id)?;
        cached.selected.is_none().then_some((id, commit))
    }

    pub fn begin_commit_selection(
        &mut self,
        id: ViewerTabId,
        commit_id: &CommitId,
    ) -> Result<
        (CommitPatchTicket, gtl_models::paths::RepositoryRoot, Commit),
        BeginCommitSelectionError,
    > {
        if self.active != Some(id) {
            return Err(BeginCommitSelectionError::StaleRange);
        }
        if self
            .tab(id)
            .is_some_and(|tab| matches!(tab.selection, CommitSelection::Pending { .. }))
        {
            return Err(BeginCommitSelectionError::SelectionPending);
        }
        let cached = self
            .cache
            .get(id)
            .cloned()
            .ok_or(BeginCommitSelectionError::StaleRange)?;
        let commit = cached
            .view
            .commits
            .iter()
            .find(|commit| &commit.id == commit_id)
            .cloned()
            .ok_or(BeginCommitSelectionError::UnknownCommit)?;
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == id)
            .ok_or(BeginCommitSelectionError::UnknownTab)?;
        if !matches!(tab.tab.state(), ViewerTabState::Ready) {
            return Err(BeginCommitSelectionError::StaleRange);
        }
        tab.selection_generation = tab.selection_generation.next();
        tab.selection = CommitSelection::Pending {
            commit: commit.clone(),
        };
        let ticket = CommitPatchTicket {
            tab_id: id,
            range_generation: tab.generation,
            selection_generation: tab.selection_generation,
        };
        let repo_root = cached.view.repo_root.clone();
        self.bump_version();
        Ok((ticket, repo_root, commit))
    }

    pub(super) fn publish_commit_patch_if_current(
        &mut self,
        ticket: CommitPatchTicket,
        patch: ViewerDiffSnapshot,
    ) -> PublishOutcome {
        if !self.commit_patch_is_current(ticket) {
            return PublishOutcome::Stale;
        }
        let Some(base) = self.cache.get(ticket.tab_id).cloned() else {
            return PublishOutcome::Stale;
        };
        let selected_id = match self.tab(ticket.tab_id).map(|tab| &tab.selection) {
            Some(CommitSelection::Pending { commit }) => commit.id.clone(),
            _ => return PublishOutcome::Stale,
        };
        let selected = base.with_selected(patch.clone());
        let transient =
            if self.cache.insert(ticket.tab_id, selected) == cache::CacheDisposition::Oversize {
                self.cache.insert(ticket.tab_id, base);
                Some(patch)
            } else {
                None
            };
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == ticket.tab_id)
        else {
            return PublishOutcome::Stale;
        };
        let CommitSelection::Pending { commit } =
            std::mem::replace(&mut tab.selection, CommitSelection::None)
        else {
            return PublishOutcome::Stale;
        };
        debug_assert_eq!(commit.id, selected_id);
        tab.selection = CommitSelection::Ready { commit, transient };
        self.bump_version();
        PublishOutcome::Published
    }

    pub(super) fn set_commit_patch_error_if_current(
        &mut self,
        ticket: CommitPatchTicket,
        reason: String,
    ) -> PublishOutcome {
        if !self.commit_patch_is_current(ticket) {
            return PublishOutcome::Stale;
        }
        let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == ticket.tab_id)
        else {
            return PublishOutcome::Stale;
        };
        let CommitSelection::Pending { commit } =
            std::mem::replace(&mut tab.selection, CommitSelection::None)
        else {
            return PublishOutcome::Stale;
        };
        tab.selection = CommitSelection::Error { commit, reason };
        self.bump_version();
        PublishOutcome::Published
    }

    pub fn clear_commit_selection(&mut self, id: ViewerTabId) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.tab.id() == id) else {
            return false;
        };
        tab.selection_generation = tab.selection_generation.next();
        tab.selection = CommitSelection::None;
        if let Some(cached) = self.cache.get(id).cloned()
            && cached.selected.is_some()
        {
            self.cache.insert(id, cached.without_selected());
        }
        self.bump_version();
        true
    }

    pub fn commit_selection_snapshot(&mut self, id: ViewerTabId) -> CommitSelectionSnapshot {
        let Some(tab) = self.tabs.iter().find(|tab| tab.tab.id() == id) else {
            return CommitSelectionSnapshot::None;
        };
        match &tab.selection {
            CommitSelection::None => CommitSelectionSnapshot::None,
            CommitSelection::Pending { commit } => CommitSelectionSnapshot::Pending {
                id: commit.id.clone(),
            },
            CommitSelection::Error { commit, reason } => CommitSelectionSnapshot::Error {
                id: commit.id.clone(),
                reason: reason.clone(),
            },
            CommitSelection::Ready { commit, transient } => {
                let view = selected_view(&mut self.cache, id, transient.as_ref());
                view.map_or_else(
                    || CommitSelectionSnapshot::Pending {
                        id: commit.id.clone(),
                    },
                    |view| CommitSelectionSnapshot::Ready {
                        id: commit.id.clone(),
                        view,
                    },
                )
            }
        }
    }

    fn commit_patch_is_current(&self, ticket: CommitPatchTicket) -> bool {
        self.tabs.iter().any(|tab| {
            tab.tab.id() == ticket.tab_id
                && tab.generation == ticket.range_generation
                && tab.selection_generation == ticket.selection_generation
                && matches!(tab.selection, CommitSelection::Pending { .. })
        })
    }

    pub(super) fn close_if_current(&mut self, ticket: ComputeTicket) -> PublishOutcome {
        if self.current_ticket(ticket.tab_id) != Some(ticket) {
            return PublishOutcome::Stale;
        }
        let closed = self.close(ticket.tab_id);
        debug_assert!(closed.is_some());
        PublishOutcome::Published
    }

    pub fn close(&mut self, id: ViewerTabId) -> Option<CloseOutcome> {
        let index = self.tabs.iter().position(|tab| tab.tab.id() == id)?;
        let outcome = if self.active == Some(id) {
            self.active = self
                .tabs
                .iter()
                .rev()
                .find(|t| t.tab.id() != id)
                .map(|t| t.tab.id());
            CloseOutcome::ActiveChanged
        } else {
            CloseOutcome::ActiveUnchanged
        };
        self.tabs.remove(index);
        self.cache.remove(id);
        self.bump_version();
        Some(outcome)
    }

    pub(crate) fn move_tab(
        &mut self,
        id: ViewerTabId,
        target_id: ViewerTabId,
        placement: ViewerTabPlacement,
    ) -> Option<MoveOutcome> {
        let from = self.tabs.iter().position(|tab| tab.tab.id() == id)?;
        let target = self.tabs.iter().position(|tab| tab.tab.id() == target_id)?;
        if from == target {
            return Some(MoveOutcome::Unchanged);
        }

        let target_slot = match placement {
            ViewerTabPlacement::Before => target,
            ViewerTabPlacement::After => target + 1,
        };
        let insertion_index = if from < target_slot {
            target_slot - 1
        } else {
            target_slot
        };
        if from == insertion_index {
            return Some(MoveOutcome::Unchanged);
        }

        let tab = self.tabs.remove(from);
        self.tabs.insert(insertion_index, tab);
        self.bump_version();
        Some(MoveOutcome::Moved)
    }

    #[must_use]
    pub fn live_source(
        &self,
        id: ViewerTabId,
    ) -> Option<(LiveSource, gtl_models::live_views::LiveComparison)> {
        let tab = self.tab(id)?;
        if tab.tab.kind() != ViewerTabKind::Live {
            return None;
        }
        let comparison = match &tab.recipe.op {
            crate::recipes::RecipeOp::Diff {
                target: crate::recipes::RecipeTarget::Base { rev },
            } if *rev == gtl_models::git::GitRevision::head() => {
                gtl_models::live_views::LiveComparison::LocalChanges
            }
            crate::recipes::RecipeOp::Diff {
                target: crate::recipes::RecipeTarget::Unpushed { .. },
            } => gtl_models::live_views::LiveComparison::UnpushedCommits,
            _ => return None,
        };
        match &tab.recipe.source {
            RecipeSource::LocalRepo(path) => {
                Some((LiveSource::local_repo(path.clone()), comparison))
            }
        }
    }

    pub fn activate(&mut self, id: ViewerTabId) -> bool {
        if !self.tabs.iter().any(|tab| tab.tab.id() == id) {
            return false;
        }
        self.active = Some(id);
        self.bump_version();
        true
    }

    #[must_use]
    pub const fn active(&self) -> Option<ViewerTabId> {
        self.active
    }

    #[must_use]
    pub fn active_content_identity(&self) -> Option<ActiveContentIdentity> {
        self.content_identity(self.active?)
    }

    #[must_use]
    pub fn content_identity(&self, tab_id: ViewerTabId) -> Option<ActiveContentIdentity> {
        let tab = self.tab(tab_id)?;
        if !matches!(tab.tab.state(), ViewerTabState::Ready)
            || matches!(tab.selection, CommitSelection::Pending { .. })
        {
            return None;
        }
        Some(ActiveContentIdentity {
            tab_id,
            range_generation: tab.generation,
            selection_generation: tab.selection_generation,
        })
    }

    #[must_use]
    pub fn active_displayed_content_identity(&self) -> Option<ActiveContentIdentity> {
        let tab = self.active.and_then(|id| self.tab(id))?;
        if !matches!(tab.tab.state(), ViewerTabState::Ready) {
            return None;
        }

        let selection_generation = match tab.selection {
            CommitSelection::Pending { .. } => tab.selection_generation.previous(),
            _ => tab.selection_generation,
        };

        Some(ActiveContentIdentity {
            tab_id: tab.tab.id(),
            range_generation: tab.generation,
            selection_generation,
        })
    }

    pub(super) fn reserve_full_context(&mut self) -> Option<ActiveContentSnapshot> {
        let snapshot = self.active_content_snapshot()?;
        if !matches!(
            snapshot.view.full_context,
            crate::diffs::FullContextDiffState::Deferred(_)
        ) || self
            .full_context_preparation
            .as_ref()
            .is_some_and(|(identity, _)| *identity == snapshot.identity)
        {
            return None;
        }
        self.full_context_preparation = Some((snapshot.identity, FullContextPreparation::Pending));
        Some(snapshot)
    }

    pub(super) fn fail_full_context_if_current(&mut self, identity: ActiveContentIdentity) {
        if self
            .full_context_preparation
            .as_ref()
            .is_some_and(|(current, _)| *current == identity)
        {
            self.full_context_preparation = Some((identity, FullContextPreparation::Failed));
            self.bump_version();
        }
    }

    pub(super) fn full_context_failed(&self, identity: ActiveContentIdentity) -> bool {
        matches!(self.full_context_preparation,
            Some((current, FullContextPreparation::Failed)) if current == identity)
    }

    pub fn active_content_snapshot(&mut self) -> Option<ActiveContentSnapshot> {
        let identity = self.active_content_identity()?;
        if let Some(view) = self.full_context_snapshot(identity) {
            return Some(ActiveContentSnapshot {
                identity,
                view: view.shared_view(),
            });
        }
        let cached = self.cache.get(identity.tab_id()).cloned()?;
        let view = match self.commit_selection_snapshot(identity.tab_id()) {
            CommitSelectionSnapshot::None | CommitSelectionSnapshot::Error { .. } => cached.view,
            CommitSelectionSnapshot::Ready { view, .. } => view,
            CommitSelectionSnapshot::Pending { .. } => return None,
        };

        Some(ActiveContentSnapshot {
            identity,
            view: view.shared_view(),
        })
    }

    pub(super) fn replace_active_content_if_current(
        &mut self,
        identity: ActiveContentIdentity,
        expected: &Arc<View>,
        replacement: ViewerDiffSnapshot,
    ) -> Option<Arc<View>> {
        if self.active_content_identity() != Some(identity) {
            return None;
        }
        let current = self.active_content_snapshot()?.shared_view();
        if !Arc::ptr_eq(&current, expected) {
            return None;
        }

        let cached = self.cache.get(identity.tab_id()).cloned()?;
        if Arc::ptr_eq(&cached.view.shared_view(), expected) {
            return Some(self.replace_cached_range(identity.tab_id(), cached, replacement));
        }

        if cached
            .selected
            .as_ref()
            .is_some_and(|selected| Arc::ptr_eq(&selected.shared_view(), expected))
        {
            return self.replace_cached_selection(identity.tab_id(), &cached, replacement);
        }

        self.replace_transient_selection(identity.tab_id(), expected, replacement)
    }

    fn replace_cached_range(
        &mut self,
        tab_id: ViewerTabId,
        cached: CachedView,
        replacement: ViewerDiffSnapshot,
    ) -> Arc<View> {
        let shared = replacement.shared_view();
        let candidate = CachedView::from_snapshot(replacement);
        let snapshot = candidate.view.clone();
        let disposition = self.cache.insert(tab_id, candidate);
        if disposition == CacheDisposition::Oversize {
            self.cache.insert(tab_id, cached);
            self.full_context_transient = self
                .active_content_identity()
                .map(|identity| (identity, snapshot));
        }
        shared
    }

    pub(super) fn full_context_snapshot(
        &self,
        identity: ActiveContentIdentity,
    ) -> Option<ViewerDiffSnapshot> {
        self.full_context_transient
            .as_ref()
            .and_then(|(current, view)| (*current == identity).then(|| view.clone()))
    }

    fn replace_cached_selection(
        &mut self,
        tab_id: ViewerTabId,
        cached: &CachedView,
        replacement: ViewerDiffSnapshot,
    ) -> Option<Arc<View>> {
        let shared = replacement.shared_view();
        let enriched = cached.with_selected(replacement.clone());
        if self.cache.insert(tab_id, enriched) != CacheDisposition::Oversize {
            return Some(shared);
        }

        self.cache.insert(tab_id, cached.without_selected());
        let tab = self.tabs.iter_mut().find(|tab| tab.tab.id() == tab_id)?;
        let CommitSelection::Ready { transient, .. } = &mut tab.selection else {
            return None;
        };
        *transient = Some(replacement);
        Some(shared)
    }

    fn replace_transient_selection(
        &mut self,
        tab_id: ViewerTabId,
        expected: &Arc<View>,
        replacement: ViewerDiffSnapshot,
    ) -> Option<Arc<View>> {
        let tab = self.tabs.iter_mut().find(|tab| tab.tab.id() == tab_id)?;
        let CommitSelection::Ready {
            transient: Some(transient),
            ..
        } = &mut tab.selection
        else {
            return None;
        };
        if !Arc::ptr_eq(&transient.shared_view(), expected) {
            return None;
        }
        let shared = replacement.shared_view();
        *transient = replacement;
        Some(shared)
    }

    #[must_use]
    pub const fn version(&self) -> ViewerVersion {
        self.version
    }

    pub fn mark_shell_changed(&mut self) {
        self.bump_version();
    }

    pub(super) fn request_focus(&mut self) {
        self.bump_version();
        self.focus_request_version = Some(self.version);
    }

    #[must_use]
    pub(super) const fn focus_request_version(&self) -> Option<ViewerVersion> {
        self.focus_request_version
    }

    fn bump_version(&mut self) {
        if self
            .full_context_preparation
            .as_ref()
            .is_some_and(|(identity, _)| {
                self.active_displayed_content_identity() != Some(*identity)
            })
        {
            self.full_context_preparation = None;
        }
        for tab in &mut self.tabs {
            if Some(tab.tab.id()) != self.active
                && let CommitSelection::Ready { transient, .. } = &mut tab.selection
            {
                *transient = None;
            }
        }
        if self
            .full_context_transient
            .as_ref()
            .is_some_and(|(identity, _)| {
                self.active_displayed_content_identity() != Some(*identity)
            })
        {
            self.full_context_transient = None;
        }
        self.version = self.version.next();
    }

    #[must_use]
    pub fn tabs(&self) -> impl ExactSizeIterator<Item = &SessionTab> {
        self.tabs.iter()
    }

    #[must_use]
    pub fn tab(&self, id: ViewerTabId) -> Option<&SessionTab> {
        self.tabs.iter().find(|tab| tab.tab.id() == id)
    }

    #[cfg(test)]
    pub fn cached_view(&mut self, id: ViewerTabId) -> Option<&CachedView> {
        self.cache.get(id)
    }

    pub fn cached_view_snapshot(&mut self, id: ViewerTabId) -> Option<CachedView> {
        self.cache.get(id).cloned()
    }

    #[must_use]
    pub fn current_ticket(&self, id: ViewerTabId) -> Option<ComputeTicket> {
        self.tabs
            .iter()
            .find(|tab| tab.tab.id() == id)
            .map(|tab| ComputeTicket {
                tab_id: id,
                generation: tab.generation,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        diffs::{Cmd, Foot, View},
        recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget},
        utils::{git_head, git_revision, project_name, repository_root},
        viewer::{ViewerTabId, ViewerTabKind},
    };

    fn cache_weight(bytes: usize) -> ViewCacheWeight {
        ViewCacheWeight::new(bytes)
    }

    fn batch_id(sequence: u64) -> RecipeBatchId {
        format!("00000000-0000-0000-0000-{sequence:012x}")
            .parse()
            .unwrap()
    }

    fn recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(repository_root("/repo")),
            op: RecipeOp::MergeDiff {
                base: None,
                pinned: None,
            },
            name: None,
        }
    }

    fn view(title: &str) -> Arc<View> {
        Arc::new(View {
            exclusions: None,
            repo_name: project_name("repo"),
            repo_root: repository_root("/repo"),
            branch: git_head("feature"),
            upstream: git_revision("main"),
            commits: Vec::new(),
            files: Vec::new(),
            title: title.into(),
            cmd: Cmd {
                lead: String::new(),
                range: "main..HEAD".into(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot { cmd: String::new() },
            full_context: crate::diffs::FullContextDiffState::Unavailable,
        })
    }

    fn ready_session() -> (ViewerSession, ViewerTabId) {
        let mut session = ViewerSession::new(cache_weight(1024));
        let id = session
            .open(recipe(), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        let ticket = session.begin_compute(id).unwrap();
        assert_eq!(
            session.publish_labeled_if_current(
                ticket,
                CachedView::new(view("current")),
                "ready".into(),
            ),
            PublishOutcome::Published
        );
        (session, id)
    }

    fn ready_session_with_commits() -> (ViewerSession, ViewerTabId, Vec<CommitId>) {
        let mut session = ViewerSession::new(cache_weight(1024 * 1024));
        let id = session
            .open(recipe(), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        let ticket = session.begin_compute(id).unwrap();
        let ids = vec![
            crate::utils::commit_id_fixture("a"),
            crate::utils::commit_id_fixture("b"),
        ];
        let mut range = (*view("range")).clone();
        range.commits = ids
            .iter()
            .enumerate()
            .map(|(index, id)| crate::utils::commit(id.as_ref(), format!("commit {index}")))
            .collect();
        session.publish_labeled_if_current(
            ticket,
            CachedView::new(Arc::new(range)),
            "ready".into(),
        );
        (session, id, ids)
    }

    #[test]
    fn cache_eviction_preserves_commit_navigation_when_the_range_reloads() {
        let (mut session, id, ids) = ready_session_with_commits();
        let range = session.cached_view(id).unwrap().without_selected();
        let (ticket, _, _) = session.begin_commit_selection(id, &ids[0]).unwrap();
        session.publish_commit_patch_if_current(ticket, ViewerDiffSnapshot::new(view("patch")));
        session.cache.remove(id);
        let ticket = session.begin_compute(id).unwrap();
        session.publish_labeled_if_current(ticket, range, "reloaded".into());
        assert!(matches!(session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Pending { id: selected } if selected == ids[0]));
        let (tab_id, commit_id) = session.selected_commit_to_reload().unwrap();
        assert_eq!((tab_id, &commit_id), (id, &ids[0]));
        let (ticket, _, _) = session.begin_commit_selection(tab_id, &commit_id).unwrap();
        assert!(session.selected_commit_to_reload().is_none());
        session.publish_commit_patch_if_current(
            ticket,
            ViewerDiffSnapshot::new(view("reloaded patch")),
        );
        assert!(session.selected_commit_to_reload().is_none());
        assert!(matches!(session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Ready { id: selected, view }
                if selected == ids[0] && view.title == "reloaded patch"));
    }

    #[test]
    fn opening_and_activating_tabs_preserves_the_selected_patch() {
        let (mut session, id, ids) = ready_session_with_commits();
        let (ticket, _, _) = session.begin_commit_selection(id, &ids[0]).unwrap();
        let patch = view("selected patch");
        session.publish_commit_patch_if_current(ticket, ViewerDiffSnapshot::new(patch.clone()));
        let identity = session.active_content_identity();
        let mut other_recipe = recipe();
        other_recipe.source = RecipeSource::LocalRepo(crate::utils::repository_root("/other"));
        let other = session
            .open(other_recipe, batch_id(2), ViewerTabKind::Snapshot)
            .unwrap();
        assert_ne!(id, other);
        assert_eq!(session.content_identity(id), identity);
        assert!(session.activate(id));
        assert_eq!(session.active_content_identity(), identity);
        assert!(matches!(session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Ready { id: selected, view }
                if selected == ids[0] && Arc::ptr_eq(&view.shared_view(), &patch)));
    }

    #[test]
    fn live_publication_preserves_a_selected_commit_until_it_leaves_the_range() {
        let (mut session, id, ids) = ready_session_with_commits();
        let (selection, _, _) = session.begin_commit_selection(id, &ids[0]).unwrap();
        let selected = view("selected patch");
        session
            .publish_commit_patch_if_current(selection, ViewerDiffSnapshot::new(selected.clone()));
        let ticket = session.current_ticket(id).unwrap();
        let original_identity = session.active_content_identity().unwrap();
        let range = session.cached_view_snapshot(id).unwrap();
        let head = gtl_models::git::GitHeadState::Commit {
            head: git_head("feature"),
            id: ids[1].clone(),
        };
        assert_eq!(
            session.publish_live_if_current(
                ticket,
                head.clone().into(),
                CachedView::new(range.view.shared_view()),
                "updated".into()
            ),
            PublishOutcome::Published
        );
        assert!(
            matches!(session.commit_selection_snapshot(id), CommitSelectionSnapshot::Ready { id: selected_id, view } if selected_id == ids[0] && Arc::ptr_eq(&view.shared_view(), &selected))
        );
        assert_ne!(
            session.active_content_identity().unwrap(),
            original_identity
        );
        let ticket = session.current_ticket(id).unwrap();
        session.publish_live_if_current(
            ticket,
            head.into(),
            CachedView::new(view("empty range")),
            "updated".into(),
        );
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::None
        ));
    }

    #[test]
    fn live_publication_rejects_work_after_a_newer_render_or_tab_switch() {
        let (mut session, id) = ready_session();
        let stale = session.current_ticket(id).unwrap();
        let head = gtl_models::git::GitHeadState::Commit {
            head: git_head("feature"),
            id: crate::utils::commit_id_fixture("a"),
        };
        session.begin_compute(id).unwrap();
        assert_eq!(
            session.publish_live_if_current(
                stale,
                head.clone().into(),
                CachedView::new(view("stale")),
                "stale".into()
            ),
            PublishOutcome::Stale
        );
        let ticket = session.current_ticket(id).unwrap();
        let mut other_recipe = recipe();
        other_recipe.name = Some(project_name("another"));
        session
            .open(other_recipe, batch_id(2), ViewerTabKind::Snapshot)
            .unwrap();
        assert_eq!(
            session.publish_live_if_current(
                ticket,
                head.into(),
                CachedView::new(view("stale")),
                "stale".into()
            ),
            PublishOutcome::Stale
        );
    }

    #[test]
    fn active_content_snapshot_shares_the_cached_view_allocation() {
        let mut session = ViewerSession::new(cache_weight(1024));
        let id = session
            .open(recipe(), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        let ticket = session.begin_compute(id).unwrap();
        let view = view("shared");
        assert_eq!(
            session.publish_labeled_if_current(
                ticket,
                CachedView::new(Arc::clone(&view)),
                "ready".into(),
            ),
            PublishOutcome::Published
        );

        let snapshot = session.active_content_snapshot().unwrap();

        assert!(Arc::ptr_eq(&view, &snapshot.shared_view()));
    }

    #[test]
    fn selected_patch_replaces_only_the_displayed_view_until_cleared() {
        let (mut session, id, ids) = ready_session_with_commits();
        let (ticket, repo_root, commit) = session.begin_commit_selection(id, &ids[0]).unwrap();

        assert_eq!(repo_root, repository_root("/repo"));
        assert_eq!(commit.id, ids[0]);
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Pending { .. }
        ));
        assert_eq!(
            session.publish_commit_patch_if_current(ticket, ViewerDiffSnapshot::new(view("patch"))),
            PublishOutcome::Published
        );
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Ready { id: selected_id, view }
                if selected_id == ids[0] && view.title == "patch"
        ));
        assert_eq!(session.cached_view(id).unwrap().view.title, "range");

        assert!(session.clear_commit_selection(id));
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::None
        ));
        assert!(session.cached_view(id).unwrap().selected.is_none());
    }

    #[test]
    fn oversized_full_source_retains_its_identity_until_the_active_generation_changes() {
        let (mut session, id) = ready_session();
        let identity = session.active_content_identity().unwrap();
        let expected = session.active_content_snapshot().unwrap().shared_view();
        let mut full = (*expected).clone();
        full.files = vec![crate::diffs::FileDiff {
            path: crate::utils::repository_relative_path("large.rs"),
            added: gtl_models::diffs::DiffLineCount::new(1),
            removed: gtl_models::diffs::DiffLineCount::default(),
            lines: vec!["+compact".into()].into(),
            full_lines: Some(vec![format!("+{}", "x".repeat(2048))].into()),
        }];
        let replacement = Arc::new(full);
        session
            .replace_active_content_if_current(
                identity,
                &expected,
                ViewerDiffSnapshot::new(Arc::clone(&replacement)),
            )
            .unwrap();
        let options = super::super::project_render_options(super::super::RenderOptions::new(
            super::super::DiffLayout::Unified,
            super::super::DiffDensity::Full,
        ));
        let retained = session.full_context_snapshot(identity).unwrap();
        let expected_id = retained.content_id(options);
        assert!(Arc::ptr_eq(
            &session.active_content_snapshot().unwrap().shared_view(),
            &replacement
        ));
        session.mark_shell_changed();
        assert_eq!(
            session
                .full_context_snapshot(identity)
                .unwrap()
                .content_id(options),
            expected_id
        );
        let active = crate::viewer::shell::project(
            &mut session,
            super::super::RenderOptions::new(
                super::super::DiffLayout::Unified,
                super::super::DiffDensity::Full,
            ),
            super::super::Theme::Dark,
            gtl_models::viewer::ViewerKeybindings::default(),
            gtl_models::viewer::ViewerSidebarVisibility::default(),
            None,
        )
        .unwrap()
        .active;
        assert!(
            matches!(active, gtl_wire::viewer::ViewerActiveState::Ready { view } if view.content_id == expected_id)
        );
        session.begin_compute(id).unwrap();
        assert!(session.full_context_transient.is_none());
    }

    #[test]
    fn active_content_replacement_updates_only_the_selected_patch() {
        let (mut session, id, ids) = ready_session_with_commits();
        let (ticket, _, _) = session.begin_commit_selection(id, &ids[0]).unwrap();
        session.publish_commit_patch_if_current(ticket, ViewerDiffSnapshot::new(view("patch")));
        let identity = session.active_content_identity().unwrap();
        let expected = session.active_content_snapshot().unwrap().shared_view();
        let replacement = view("patch full");

        let published = session
            .replace_active_content_if_current(
                identity,
                &expected,
                ViewerDiffSnapshot::new(Arc::clone(&replacement)),
            )
            .unwrap();

        assert!(Arc::ptr_eq(&published, &replacement));
        assert_eq!(session.cached_view(id).unwrap().view.title, "range");
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Ready { view, .. }
                if Arc::ptr_eq(&view.shared_view(), &replacement)
        ));
        assert!(session.clear_commit_selection(id));
        assert_eq!(
            session.active_content_snapshot().unwrap().view().title,
            "range"
        );
    }

    #[test]
    fn active_content_replacement_rejects_a_refreshed_identity() {
        let (mut session, id) = ready_session();
        let identity = session.active_content_identity().unwrap();
        let expected = session.active_content_snapshot().unwrap().shared_view();
        session.refresh(id).unwrap();

        assert!(
            session
                .replace_active_content_if_current(
                    identity,
                    &expected,
                    ViewerDiffSnapshot::new(view("stale"))
                )
                .is_none()
        );
    }

    #[test]
    fn a_pending_commit_selection_rejects_another_reservation_until_cleared() {
        let (mut session, id, ids) = ready_session_with_commits();
        let (first, _, _) = session.begin_commit_selection(id, &ids[0]).unwrap();

        assert_eq!(
            session.begin_commit_selection(id, &ids[1]),
            Err(BeginCommitSelectionError::SelectionPending)
        );

        assert!(session.clear_commit_selection(id));

        assert_eq!(
            session.publish_commit_patch_if_current(first, ViewerDiffSnapshot::new(view("first"))),
            PublishOutcome::Stale
        );
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::None
        ));
    }

    #[test]
    fn commit_selection_requires_an_exact_cached_identity() {
        let (mut session, id, _) = ready_session_with_commits();
        let unknown_id = crate::utils::commit_id_fixture("c");

        assert_eq!(
            session.begin_commit_selection(id, &unknown_id),
            Err(BeginCommitSelectionError::UnknownCommit)
        );
    }

    #[test]
    fn content_identities_track_selection_and_refresh_transitions() {
        let (mut session, id, ids) = ready_session_with_commits();
        let range = session.active_content_identity().unwrap();
        assert_eq!(session.active_displayed_content_identity(), Some(range));

        let (first_selection, _, _) = session.begin_commit_selection(id, &ids[0]).unwrap();

        assert!(session.active_content_identity().is_none());
        assert_eq!(session.active_displayed_content_identity(), Some(range));
        assert_eq!(
            session.publish_commit_patch_if_current(
                first_selection,
                ViewerDiffSnapshot::new(view("selected"))
            ),
            PublishOutcome::Published
        );
        let selected = session.active_content_identity().unwrap();
        assert_eq!(selected.tab_id(), range.tab_id());
        assert_eq!(selected.range_generation(), range.range_generation());
        assert_ne!(
            selected.selection_generation(),
            range.selection_generation()
        );
        assert_eq!(session.active_displayed_content_identity(), Some(selected));

        let (second_selection, _, _) = session.begin_commit_selection(id, &ids[1]).unwrap();
        assert!(session.active_content_identity().is_none());
        assert_eq!(session.active_displayed_content_identity(), Some(selected));
        assert_eq!(
            session.set_commit_patch_error_if_current(second_selection, "failed".into()),
            PublishOutcome::Published
        );
        let selection_error = session.active_content_identity().unwrap();
        assert_eq!(selection_error.tab_id(), range.tab_id());
        assert_eq!(selection_error.range_generation(), range.range_generation());
        assert_ne!(
            selection_error.selection_generation(),
            selected.selection_generation()
        );
        assert_eq!(
            session.active_displayed_content_identity(),
            Some(selection_error)
        );

        let refresh = session.begin_compute(id).unwrap();
        assert!(session.active_content_identity().is_none());
        assert!(session.active_displayed_content_identity().is_none());
        assert_eq!(
            session.publish_labeled_if_current(
                refresh,
                CachedView::new(view("refreshed")),
                "refreshed".into(),
            ),
            PublishOutcome::Published
        );
        let refreshed = session.active_content_identity().unwrap();
        assert_eq!(refreshed.tab_id(), range.tab_id());
        assert_ne!(refreshed.range_generation(), range.range_generation());
    }

    #[test]
    fn reopening_a_recipe_reuses_its_tab_and_updates_the_batch() {
        let mut session = ViewerSession::new(cache_weight(128 * 1024 * 1024));
        let first = session
            .open(recipe(), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        let second = session
            .open(recipe(), batch_id(2), ViewerTabKind::Snapshot)
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(session.tab(first).unwrap().batch_id, batch_id(2));
    }

    #[test]
    fn reopening_a_recipe_updates_its_authoritative_kind() {
        let mut session = ViewerSession::new(cache_weight(1024));
        let id = session
            .open(recipe(), batch_id(3), ViewerTabKind::Snapshot)
            .unwrap();

        let reopened = session
            .open(recipe(), batch_id(4), ViewerTabKind::Live)
            .unwrap();

        assert_eq!(reopened, id);
        let tab = session.tab(id).unwrap();
        assert_eq!(tab.tab.kind(), ViewerTabKind::Live);
        assert_eq!(tab.batch_id, batch_id(4));
    }

    #[test]
    fn stale_compute_cannot_overwrite_a_newer_refresh() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).unwrap();
        let current = session.begin_compute(id).unwrap();

        assert_eq!(
            session.publish_labeled_if_current(
                current,
                CachedView::new(view("newer")),
                "newer".into(),
            ),
            PublishOutcome::Published
        );
        assert_eq!(
            session.publish_labeled_if_current(
                stale,
                CachedView::new(view("stale")),
                "stale".into(),
            ),
            PublishOutcome::Stale
        );
        assert_eq!(session.cached_view(id).unwrap().view.title, "newer");
    }

    #[test]
    fn close_if_current_rejects_a_stale_compute() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).unwrap();
        let current = session.begin_compute(id).unwrap();

        assert_eq!(session.close_if_current(stale), PublishOutcome::Stale);
        assert!(session.tab(id).is_some());
        assert_eq!(session.close_if_current(current), PublishOutcome::Published);
        assert!(session.tab(id).is_none());
    }

    #[test]
    fn refresh_invalidates_cached_content_and_older_tickets() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).unwrap();

        let refresh = session.refresh(id).unwrap();

        assert!(session.cached_view(id).is_none());
        assert_eq!(
            session.publish_labeled_if_current(
                stale,
                CachedView::new(view("stale")),
                "stale".into(),
            ),
            PublishOutcome::Stale
        );
        assert_eq!(
            session.publish_labeled_if_current(
                refresh,
                CachedView::new(view("fresh")),
                "fresh".into(),
            ),
            PublishOutcome::Published
        );
    }

    #[test]
    fn beginning_reopen_compute_invalidates_old_view_before_error_publication() {
        let (mut session, id) = ready_session();
        assert!(session.cached_view(id).is_some());

        let ticket = session.begin_compute(id).unwrap();
        assert!(session.cached_view(id).is_none());
        assert_eq!(
            session.tab(id).unwrap().tab.state(),
            &ViewerTabState::Error {
                reason: RENDER_PENDING_REASON.into(),
            }
        );
        session.set_state_if_current(
            ticket,
            ViewerTabState::Error {
                reason: "new failure".into(),
            },
        );

        assert!(session.cached_view(id).is_none());
        assert_eq!(
            session.tab(id).unwrap().tab.state(),
            &ViewerTabState::Error {
                reason: "new failure".into()
            }
        );
    }

    #[test]
    fn shell_visible_mutations_advance_the_session_revision() {
        let mut session = ViewerSession::new(cache_weight(1024));
        let start = session.version();
        let id = session
            .open(recipe(), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        assert!(session.version() > start);
        let opened = session.version();
        assert!(session.activate(id));
        assert!(session.version() > opened);
        let ticket = session.begin_compute(id).unwrap();
        let pending = session.version();
        session.set_state_if_current(
            ticket,
            ViewerTabState::Error {
                reason: "safe".into(),
            },
        );
        assert!(session.version() > pending);
        let failed = session.version();
        assert_eq!(session.close(id), Some(CloseOutcome::ActiveChanged));
        assert!(session.version() > failed);
    }

    #[test]
    fn close_invalidates_an_outstanding_compute() {
        let (mut session, id) = ready_session();
        let ticket = session.begin_compute(id).unwrap();

        assert_eq!(session.close(id), Some(CloseOutcome::ActiveChanged));
        assert_eq!(
            session.publish_labeled_if_current(
                ticket,
                CachedView::new(view("late")),
                "late".into(),
            ),
            PublishOutcome::Stale
        );
        assert!(session.cached_view(id).is_none());
    }

    #[test]
    fn activating_a_tab_updates_the_active_identity() {
        let mut session = ViewerSession::new(cache_weight(1024));
        let first = session
            .open(recipe(), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        let mut other = recipe();
        other.source = RecipeSource::LocalRepo(repository_root("/other"));
        let second = session
            .open(other, batch_id(1), ViewerTabKind::Live)
            .unwrap();

        assert!(session.activate(first));
        assert_eq!(session.active(), Some(first));
        assert!(session.activate(second));
        assert_eq!(session.active(), Some(second));
    }

    #[test]
    fn session_can_be_guarded_and_shared_across_tauri_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<std::sync::Mutex<ViewerSession>>();
    }

    fn pinned_unpushed_recipe(head: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(repository_root("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(crate::utils::pinned_range("a", head)),
                },
            },
            name: None,
        }
    }

    #[test]
    fn open_dedupes_snapshot_tabs_by_unpinned_identity_and_adopts_the_new_pin() {
        let mut session = ViewerSession::new(cache_weight(1024 * 1024));
        let first = session
            .open(
                pinned_unpushed_recipe("b"),
                batch_id(1),
                ViewerTabKind::Snapshot,
            )
            .unwrap();
        let second = session
            .open(
                pinned_unpushed_recipe("c"),
                batch_id(2),
                ViewerTabKind::Snapshot,
            )
            .unwrap();

        assert_eq!(first, second, "same repo+op must reuse the tab across pins");
        let tab = session.tab(second).unwrap();
        assert_eq!(
            tab.recipe,
            pinned_unpushed_recipe("c"),
            "the newest pin wins the tab"
        );
    }

    #[test]
    fn open_keeps_distinct_symbolic_intents_as_distinct_tabs() {
        let mut session = ViewerSession::new(cache_weight(1024 * 1024));
        let range_recipe = |range: &str| Recipe {
            source: RecipeSource::LocalRepo(repository_root("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: gtl_models::git::GitRange::try_new(range.to_owned()).unwrap(),
                    pinned: None,
                },
            },
            name: None,
        };
        let a = session
            .open(range_recipe("a..b"), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        let b = session
            .open(range_recipe("c..d"), batch_id(2), ViewerTabKind::Snapshot)
            .unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn moving_a_tab_preserves_identity_and_active_state() {
        let mut session = ViewerSession::new(cache_weight(1024));
        let open = |session: &mut ViewerSession, range: &str, batch| {
            let recipe = Recipe {
                source: RecipeSource::LocalRepo(repository_root("/repo")),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Range {
                        range: gtl_models::git::GitRange::try_new(range.to_owned()).unwrap(),
                        pinned: None,
                    },
                },
                name: None,
            };
            session
                .open(recipe, batch_id(batch), ViewerTabKind::Snapshot)
                .unwrap()
        };
        let first = open(&mut session, "a..b", 1);
        let second = open(&mut session, "c..d", 2);
        let third = open(&mut session, "e..f", 3);
        let fourth = open(&mut session, "g..h", 4);
        assert!(session.activate(second));
        let version = session.version();

        assert_eq!(
            session.move_tab(first, third, ViewerTabPlacement::After),
            Some(MoveOutcome::Moved)
        );
        assert_eq!(
            session.tabs().map(|tab| tab.tab.id()).collect::<Vec<_>>(),
            vec![second, third, first, fourth]
        );
        assert_eq!(session.active(), Some(second));
        assert!(session.version() > version);
    }

    #[test]
    fn moving_a_tab_to_its_current_slot_is_idempotent() {
        let mut session = ViewerSession::new(cache_weight(1024));
        let first = session
            .open(recipe(), batch_id(1), ViewerTabKind::Snapshot)
            .unwrap();
        let mut second_recipe = recipe();
        second_recipe.name = Some(project_name("second"));
        let second = session
            .open(second_recipe, batch_id(2), ViewerTabKind::Snapshot)
            .unwrap();
        assert_ne!(first, second);
        let version = session.version();

        assert_eq!(
            session.move_tab(first, second, ViewerTabPlacement::Before),
            Some(MoveOutcome::Unchanged)
        );
        assert_eq!(session.version(), version);
    }
}
