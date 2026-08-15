mod cache;
mod pending;

use std::{path::PathBuf, sync::Arc};

#[cfg(feature = "benchmark-support")]
pub use cache::{CacheDisposition, CachedView, WeightedViewCache};
#[cfg(not(feature = "benchmark-support"))]
pub(crate) use cache::{CachedView, WeightedViewCache};
use gtl_application::{
    diffs::View,
    viewer::initial_recipe_label::{self, InitialRecipeLabel},
};
use gtl_models::{
    diffs::{Commit, CommitId},
    live_views::LiveSource,
    viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState},
};
use gtl_wire::recipes::{Recipe, RecipeSource};
pub(crate) use pending::{PendingRecipes, PendingRecipesError};

/// Marks a tab whose view is still being computed.
pub(crate) const RENDER_PENDING_REASON: &str = "render pending";

/// A generation token authorizing publication for one still-current compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComputeTicket {
    pub(crate) tab_id: ViewerTabId,
    pub(crate) generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommitPatchTicket {
    pub(crate) tab_id: ViewerTabId,
    pub(crate) range_generation: u64,
    pub(crate) selection_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActiveContentIdentity {
    tab_id: ViewerTabId,
    range_generation: u64,
    selection_generation: u64,
}

impl ActiveContentIdentity {
    pub(crate) const fn tab_id(self) -> ViewerTabId {
        self.tab_id
    }

    pub(crate) const fn range_generation(self) -> u64 {
        self.range_generation
    }

    pub(crate) const fn selection_generation(self) -> u64 {
        self.selection_generation
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ActiveContentSnapshot {
    identity: ActiveContentIdentity,
    view: Arc<View>,
}

impl ActiveContentSnapshot {
    pub(crate) const fn identity(&self) -> ActiveContentIdentity {
        self.identity
    }

    pub(crate) fn view(&self) -> &View {
        &self.view
    }

    pub(crate) fn shared_view(&self) -> Arc<View> {
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
        transient: Option<Arc<View>>,
    },
    Error {
        commit: Commit,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum CommitSelectionSnapshot {
    None,
    Pending { id: CommitId },
    Ready { id: CommitId, view: Arc<View> },
    Error { id: CommitId, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BeginCommitSelectionError {
    UnknownTab,
    StaleRange,
    UnknownCommit,
    SelectionPending,
}

/// Whether a compute result was current enough to mutate the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublishOutcome {
    Published,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseOutcome {
    ActiveChanged,
    ActiveUnchanged,
}

/// Session-owned metadata for one recipe tab.
#[derive(Debug, Clone)]
pub(crate) struct SessionTab {
    pub(crate) tab: ViewerTab,
    pub(crate) recipe: Recipe,
    pub(crate) batch_id: String,
    generation: u64,
    selection_generation: u64,
    selection: CommitSelection,
}

/// Authoritative recipe tabs plus their separately bounded computed views.
pub(crate) struct ViewerSession {
    tabs: Vec<SessionTab>,
    cache: WeightedViewCache,
    active: Option<ViewerTabId>,
    next_id: Option<u64>,
    revision: u64,
}

impl ViewerSession {
    pub(crate) fn new(max_cache_weight: usize) -> Self {
        Self {
            tabs: Vec::new(),
            cache: WeightedViewCache::new(max_cache_weight),
            active: None,
            next_id: Some(1),
            revision: 0,
        }
    }

    pub(crate) fn open(
        &mut self,
        recipe: Recipe,
        batch_id: String,
        kind: ViewerTabKind,
    ) -> Option<ViewerTabId> {
        let label = initial_recipe_label::execute(InitialRecipeLabel {
            recipe: recipe.clone(),
        })
        .label;
        self.open_labeled(recipe, batch_id, kind, label)
    }

    fn open_labeled(
        &mut self,
        recipe: Recipe,
        batch_id: String,
        kind: ViewerTabKind,
        label: String,
    ) -> Option<ViewerTabId> {
        self.clear_every_commit_selection();
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
            self.bump_revision();
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
            generation: 0,
            selection_generation: 0,
            selection: CommitSelection::None,
        });
        self.active = Some(id);
        self.bump_revision();
        Some(id)
    }

    pub(crate) fn begin_compute(&mut self, id: ViewerTabId) -> Option<ComputeTicket> {
        let tab = self.tabs.iter_mut().find(|tab| tab.tab.id() == id)?;
        self.cache.remove(id);
        // Tickets are process-local and short-lived; wrapping would require 2^64 mutations while
        // one ticket remains in flight before an old ticket could compare equal again.
        tab.generation = tab.generation.wrapping_add(1);
        tab.selection_generation = tab.selection_generation.wrapping_add(1);
        tab.selection = CommitSelection::None;
        tab.tab = ViewerTab::new(
            id,
            tab.tab.label().into(),
            tab.tab.kind(),
            ViewerTabState::Error {
                reason: RENDER_PENDING_REASON.into(),
            },
        );
        let generation = tab.generation;
        self.bump_revision();
        Some(ComputeTicket {
            tab_id: id,
            generation,
        })
    }

    pub(crate) fn publish_labeled_if_current(
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

        tab.tab = ViewerTab::new(ticket.tab_id, label, tab.tab.kind(), ViewerTabState::Ready);
        self.cache.insert(ticket.tab_id, value);
        self.bump_revision();
        PublishOutcome::Published
    }

    pub(crate) fn set_state_if_current(
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
        self.bump_revision();
        PublishOutcome::Published
    }

    pub(crate) fn refresh(&mut self, id: ViewerTabId) -> Option<ComputeTicket> {
        self.begin_compute(id)
    }

    pub(crate) fn begin_commit_selection(
        &mut self,
        id: ViewerTabId,
        commit_id: &CommitId,
    ) -> Result<(CommitPatchTicket, PathBuf, Commit), BeginCommitSelectionError> {
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
        tab.selection_generation = tab.selection_generation.wrapping_add(1);
        tab.selection = CommitSelection::Pending {
            commit: commit.clone(),
        };
        let ticket = CommitPatchTicket {
            tab_id: id,
            range_generation: tab.generation,
            selection_generation: tab.selection_generation,
        };
        let repo_root = PathBuf::from(&cached.view.repo_root);
        self.bump_revision();
        Ok((ticket, repo_root, commit))
    }

    pub(crate) fn publish_commit_patch_if_current(
        &mut self,
        ticket: CommitPatchTicket,
        patch: Arc<View>,
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
        let selected = base.with_selected(Arc::clone(&patch));
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
        self.bump_revision();
        PublishOutcome::Published
    }

    pub(crate) fn set_commit_patch_error_if_current(
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
        self.bump_revision();
        PublishOutcome::Published
    }

    pub(crate) fn clear_commit_selection(&mut self, id: ViewerTabId) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.tab.id() == id) else {
            return false;
        };
        tab.selection_generation = tab.selection_generation.wrapping_add(1);
        tab.selection = CommitSelection::None;
        if let Some(cached) = self.cache.get(id).cloned()
            && cached.selected.is_some()
        {
            self.cache.insert(id, cached.without_selected());
        }
        self.bump_revision();
        true
    }

    pub(crate) fn commit_selection_snapshot(&mut self, id: ViewerTabId) -> CommitSelectionSnapshot {
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
                let view = transient.clone().or_else(|| {
                    self.cache
                        .get(id)
                        .and_then(|cached| cached.selected.clone())
                });
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

    pub(crate) fn close_if_current(&mut self, ticket: ComputeTicket) -> PublishOutcome {
        if self.current_ticket(ticket.tab_id) != Some(ticket) {
            return PublishOutcome::Stale;
        }
        let closed = self.close(ticket.tab_id);
        debug_assert!(closed.is_some());
        PublishOutcome::Published
    }

    pub(crate) fn close(&mut self, id: ViewerTabId) -> Option<CloseOutcome> {
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
        self.bump_revision();
        Some(outcome)
    }

    pub(crate) fn live_source(&self, id: ViewerTabId) -> Option<LiveSource> {
        let tab = self.tab(id)?;
        if tab.tab.kind() != ViewerTabKind::Live {
            return None;
        }
        match &tab.recipe.source {
            RecipeSource::LocalRepo(path) => Some(LiveSource::local_repo(path.clone())),
        }
    }

    pub(crate) fn close_live_view(&mut self, id: ViewerTabId, source: &LiveSource) -> bool {
        if self.live_source(id).as_ref() != Some(source) {
            return false;
        }
        self.close(id).is_some()
    }

    pub(crate) fn activate(&mut self, id: ViewerTabId) -> bool {
        if self.tabs.iter().any(|tab| tab.tab.id() == id) {
            if self.active != Some(id) {
                self.clear_every_commit_selection();
            }
            self.active = Some(id);
            self.bump_revision();
            true
        } else {
            false
        }
    }

    pub(crate) const fn active(&self) -> Option<ViewerTabId> {
        self.active
    }

    pub(crate) fn active_content_identity(&self) -> Option<ActiveContentIdentity> {
        let identity = self.active_displayed_content_identity()?;
        let tab = self.tab(identity.tab_id())?;
        if matches!(tab.selection, CommitSelection::Pending { .. }) {
            return None;
        }

        Some(identity)
    }

    pub(crate) fn active_displayed_content_identity(&self) -> Option<ActiveContentIdentity> {
        let tab = self.active.and_then(|id| self.tab(id))?;
        if !matches!(tab.tab.state(), ViewerTabState::Ready) {
            return None;
        }

        let selection_generation = match tab.selection {
            CommitSelection::Pending { .. } => tab.selection_generation.wrapping_sub(1),
            _ => tab.selection_generation,
        };

        Some(ActiveContentIdentity {
            tab_id: tab.tab.id(),
            range_generation: tab.generation,
            selection_generation,
        })
    }

    pub(crate) fn active_content_snapshot(&mut self) -> Option<ActiveContentSnapshot> {
        let identity = self.active_content_identity()?;
        let cached = self.cache.get(identity.tab_id()).cloned()?;
        let view = match self.commit_selection_snapshot(identity.tab_id()) {
            CommitSelectionSnapshot::None | CommitSelectionSnapshot::Error { .. } => cached.view,
            CommitSelectionSnapshot::Ready { view, .. } => view,
            CommitSelectionSnapshot::Pending { .. } => return None,
        };

        Some(ActiveContentSnapshot { identity, view })
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    fn clear_every_commit_selection(&mut self) {
        let selected = self
            .tabs
            .iter()
            .filter(|tab| !matches!(tab.selection, CommitSelection::None))
            .map(|tab| tab.tab.id())
            .collect::<Vec<_>>();
        for id in selected {
            self.clear_commit_selection(id);
        }
    }

    pub(crate) fn tabs(&self) -> impl ExactSizeIterator<Item = &SessionTab> {
        self.tabs.iter()
    }

    pub(crate) fn tab(&self, id: ViewerTabId) -> Option<&SessionTab> {
        self.tabs.iter().find(|tab| tab.tab.id() == id)
    }

    #[cfg(test)]
    pub(crate) fn cached_view(&mut self, id: ViewerTabId) -> Option<&CachedView> {
        self.cache.get(id)
    }

    pub(crate) fn cached_view_snapshot(&mut self, id: ViewerTabId) -> Option<CachedView> {
        self.cache.get(id).cloned()
    }

    pub(crate) fn current_ticket(&self, id: ViewerTabId) -> Option<ComputeTicket> {
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
    use std::{path::PathBuf, sync::Arc};

    use gtl_application::{
        diffs::{Cmd, Foot, View},
        viewer::{ViewerTabId, ViewerTabKind},
    };
    use gtl_wire::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

    use super::*;

    fn recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(PathBuf::from("/repo")),
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
            repo_name: "repo".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "main".into(),
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
        })
    }

    fn ready_session() -> (ViewerSession, ViewerTabId) {
        let mut session = ViewerSession::new(1024);
        let id = session
            .open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let ticket = session.begin_compute(id).expect("tab exists");
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
        let mut session = ViewerSession::new(1024 * 1024);
        let id = session
            .open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let ticket = session.begin_compute(id).expect("tab exists");
        let ids = vec![
            crate::testing::commit_id("a"),
            crate::testing::commit_id("b"),
        ];
        let mut range = (*view("range")).clone();
        range.commits = ids
            .iter()
            .enumerate()
            .map(|(index, id)| crate::testing::commit(id.as_ref(), format!("commit {index}")))
            .collect();
        session.publish_labeled_if_current(
            ticket,
            CachedView::new(Arc::new(range)),
            "ready".into(),
        );
        (session, id, ids)
    }

    #[test]
    fn active_content_snapshot_shares_the_cached_view_allocation() {
        let mut session = ViewerSession::new(1024);
        let id = session
            .open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let ticket = session.begin_compute(id).expect("tab exists");
        let view = view("shared");
        assert_eq!(
            session.publish_labeled_if_current(
                ticket,
                CachedView::new(Arc::clone(&view)),
                "ready".into(),
            ),
            PublishOutcome::Published
        );

        let snapshot = session.active_content_snapshot().expect("active snapshot");

        assert!(Arc::ptr_eq(&view, &snapshot.shared_view()));
    }

    #[test]
    fn selected_patch_replaces_only_the_displayed_view_until_cleared() {
        let (mut session, id, ids) = ready_session_with_commits();
        let (ticket, repo_root, commit) = session
            .begin_commit_selection(id, &ids[0])
            .expect("cached commit can be selected");

        assert_eq!(repo_root, PathBuf::from("/repo"));
        assert_eq!(commit.id, ids[0]);
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Pending { .. }
        ));
        assert_eq!(
            session.publish_commit_patch_if_current(ticket, view("patch")),
            PublishOutcome::Published
        );
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::Ready { id: selected_id, view }
                if selected_id == ids[0] && view.title == "patch"
        ));
        assert_eq!(
            session
                .cached_view(id)
                .expect("range remains cached")
                .view
                .title,
            "range"
        );

        assert!(session.clear_commit_selection(id));
        assert!(matches!(
            session.commit_selection_snapshot(id),
            CommitSelectionSnapshot::None
        ));
        assert!(
            session
                .cached_view(id)
                .expect("range remains cached")
                .selected
                .is_none()
        );
    }

    #[test]
    fn a_pending_commit_selection_rejects_another_reservation_until_cleared() {
        let (mut session, id, ids) = ready_session_with_commits();
        let (first, _, _) = session
            .begin_commit_selection(id, &ids[0])
            .expect("first selection");

        assert_eq!(
            session.begin_commit_selection(id, &ids[1]),
            Err(BeginCommitSelectionError::SelectionPending)
        );

        assert!(session.clear_commit_selection(id));

        assert_eq!(
            session.publish_commit_patch_if_current(first, view("first")),
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
        let unknown_id = crate::testing::commit_id("c");

        assert_eq!(
            session.begin_commit_selection(id, &unknown_id),
            Err(BeginCommitSelectionError::UnknownCommit)
        );
    }

    #[test]
    fn content_identities_track_selection_and_refresh_transitions() {
        let (mut session, id, ids) = ready_session_with_commits();
        let range = session
            .active_content_identity()
            .expect("range content is ready");
        assert_eq!(session.active_displayed_content_identity(), Some(range));

        let (first_selection, _, _) = session
            .begin_commit_selection(id, &ids[0])
            .expect("commit can be selected");

        assert!(session.active_content_identity().is_none());
        assert_eq!(session.active_displayed_content_identity(), Some(range));
        assert_eq!(
            session.publish_commit_patch_if_current(first_selection, view("selected")),
            PublishOutcome::Published
        );
        let selected = session
            .active_content_identity()
            .expect("selected commit content is ready");
        assert_eq!(selected.tab_id(), range.tab_id());
        assert_eq!(selected.range_generation(), range.range_generation());
        assert_ne!(
            selected.selection_generation(),
            range.selection_generation()
        );
        assert_eq!(session.active_displayed_content_identity(), Some(selected));

        let (second_selection, _, _) = session
            .begin_commit_selection(id, &ids[1])
            .expect("another commit can be selected");
        assert!(session.active_content_identity().is_none());
        assert_eq!(session.active_displayed_content_identity(), Some(selected));
        assert_eq!(
            session.set_commit_patch_error_if_current(second_selection, "failed".into()),
            PublishOutcome::Published
        );
        let selection_error = session
            .active_content_identity()
            .expect("selection error displays range content");
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

        let refresh = session.begin_compute(id).expect("tab exists");
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
        let refreshed = session
            .active_content_identity()
            .expect("refreshed content is ready");
        assert_eq!(refreshed.tab_id(), range.tab_id());
        assert_ne!(refreshed.range_generation(), range.range_generation());
    }

    #[test]
    fn reopening_a_recipe_reuses_its_tab_and_updates_the_batch() {
        let mut session = ViewerSession::new(128 * 1024 * 1024);
        let first = session
            .open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let second = session
            .open(recipe(), "batch-2".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");

        assert_eq!(first, second);
        assert_eq!(session.tab(first).expect("tab").batch_id, "batch-2");
    }

    #[test]
    fn reopening_a_recipe_updates_its_authoritative_kind() {
        let mut session = ViewerSession::new(1024);
        let id = session
            .open(recipe(), "snapshot".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");

        let reopened = session
            .open(recipe(), "live".into(), ViewerTabKind::Live)
            .expect("tab id should be available");

        assert_eq!(reopened, id);
        let tab = session.tab(id).expect("tab");
        assert_eq!(tab.tab.kind(), ViewerTabKind::Live);
        assert_eq!(tab.batch_id, "live");
    }

    #[test]
    fn stale_compute_cannot_overwrite_a_newer_refresh() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).expect("tab exists");
        let current = session.begin_compute(id).expect("tab exists");

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
        assert_eq!(
            session.cached_view(id).expect("current view").view.title,
            "newer"
        );
    }

    #[test]
    fn close_if_current_rejects_a_stale_compute() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).expect("stale ticket");
        let current = session.begin_compute(id).expect("current ticket");

        assert_eq!(session.close_if_current(stale), PublishOutcome::Stale);
        assert!(session.tab(id).is_some());
        assert_eq!(session.close_if_current(current), PublishOutcome::Published);
        assert!(session.tab(id).is_none());
    }

    #[test]
    fn refresh_invalidates_cached_content_and_older_tickets() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).expect("tab exists");

        let refresh = session.refresh(id).expect("tab exists");

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

        let ticket = session.begin_compute(id).expect("reopen ticket");
        assert!(session.cached_view(id).is_none());
        assert_eq!(
            session.tab(id).expect("tab").tab.state(),
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
            session.tab(id).expect("tab").tab.state(),
            &ViewerTabState::Error {
                reason: "new failure".into()
            }
        );
    }

    #[test]
    fn shell_visible_mutations_advance_the_session_revision() {
        let mut session = ViewerSession::new(1024);
        let start = session.revision();
        let id = session
            .open(recipe(), "batch".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        assert!(session.revision() > start);
        let opened = session.revision();
        assert!(session.activate(id));
        assert!(session.revision() > opened);
        let ticket = session.begin_compute(id).expect("ticket");
        let pending = session.revision();
        session.set_state_if_current(
            ticket,
            ViewerTabState::Error {
                reason: "safe".into(),
            },
        );
        assert!(session.revision() > pending);
        let failed = session.revision();
        assert_eq!(session.close(id), Some(CloseOutcome::ActiveChanged));
        assert!(session.revision() > failed);
    }

    #[test]
    fn close_invalidates_an_outstanding_compute() {
        let (mut session, id) = ready_session();
        let ticket = session.begin_compute(id).expect("tab exists");

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
        let mut session = ViewerSession::new(1024);
        let first = session
            .open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot)
            .expect("tab id should be available");
        let mut other = recipe();
        other.source = RecipeSource::LocalRepo(PathBuf::from("/other"));
        let second = session
            .open(other, "batch-1".into(), ViewerTabKind::Live)
            .expect("tab id should be available");

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
            source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed {
                    pinned: Some(crate::testing::pinned_range("a", head)),
                },
            },
            name: None,
        }
    }

    #[test]
    fn open_dedupes_snapshot_tabs_by_unpinned_identity_and_adopts_the_new_pin() {
        let mut session = ViewerSession::new(1024 * 1024);
        let first = session
            .open(
                pinned_unpushed_recipe("b"),
                "batch-1".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("tab id should be available");
        let second = session
            .open(
                pinned_unpushed_recipe("c"),
                "batch-2".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("tab id should be available");

        assert_eq!(first, second, "same repo+op must reuse the tab across pins");
        let tab = session.tab(second).expect("tab exists");
        assert_eq!(
            tab.recipe,
            pinned_unpushed_recipe("c"),
            "the newest pin wins the tab"
        );
    }

    #[test]
    fn open_keeps_distinct_symbolic_intents_as_distinct_tabs() {
        let mut session = ViewerSession::new(1024 * 1024);
        let range_recipe = |range: &str| Recipe {
            source: RecipeSource::LocalRepo(PathBuf::from("/repos/gt")),
            op: RecipeOp::Diff {
                target: RecipeTarget::Range {
                    range: range.into(),
                    pinned: None,
                },
            },
            name: None,
        };
        let a = session
            .open(
                range_recipe("a..b"),
                "batch-1".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("tab id should be available");
        let b = session
            .open(
                range_recipe("c..d"),
                "batch-2".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("tab id should be available");
        assert_ne!(a, b);
    }
}
