mod cache;
mod pending;

pub(crate) use cache::CachedView;
use cache::WeightedViewCache;
use domain::{
    live_views::LiveSource,
    viewer::{RenderOptions, Theme, ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState},
};
use gtl_recipe::{Recipe, RecipeSource};
pub(crate) use pending::{PendingRecipes, PendingRecipesError};

use crate::tab_label;

/// A generation token authorizing publication for one still-current compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ComputeTicket {
    pub(crate) tab_id: ViewerTabId,
    pub(crate) generation: u64,
}

/// Whether a compute result was current enough to mutate the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublishOutcome {
    Published,
    Stale,
}

/// Session-owned metadata for one recipe tab.
#[derive(Debug, Clone)]
pub(crate) struct SessionTab {
    pub(crate) tab: ViewerTab,
    pub(crate) recipe: Recipe,
    pub(crate) batch_id: String,
    generation: u64,
}

/// Authoritative recipe tabs plus their separately bounded computed views.
pub(crate) struct ViewerSession {
    tabs: Vec<SessionTab>,
    cache: WeightedViewCache,
    active: Option<ViewerTabId>,
    next_id: u64,
    revision: u64,
}

impl ViewerSession {
    pub(crate) fn new(max_cache_weight: usize) -> Self {
        Self {
            tabs: Vec::new(),
            cache: WeightedViewCache::new(max_cache_weight),
            active: None,
            next_id: 1,
            revision: 0,
        }
    }

    pub(crate) fn open(
        &mut self,
        recipe: Recipe,
        batch_id: String,
        kind: ViewerTabKind,
    ) -> ViewerTabId {
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
            return id;
        }

        let id = ViewerTabId::try_new(self.next_id).expect("session tab IDs start at one");
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("viewer tab ID exhausted");
        let label = tab_label::initial(&recipe);
        self.tabs.push(SessionTab {
            tab: ViewerTab::new(
                id,
                label,
                kind,
                ViewerTabState::Error {
                    reason: "render pending".into(),
                },
            ),
            recipe,
            batch_id,
            generation: 0,
        });
        self.active = Some(id);
        self.bump_revision();
        id
    }

    pub(crate) fn begin_compute(&mut self, id: ViewerTabId) -> Option<ComputeTicket> {
        if !self.tabs.iter().any(|tab| tab.tab.id() == id) {
            return None;
        }
        self.cache.remove(id);
        let tab = self
            .tabs
            .iter_mut()
            .find(|tab| tab.tab.id() == id)
            .expect("tab existence checked before cache invalidation");
        tab.generation = tab
            .generation
            .checked_add(1)
            .expect("tab compute generation exhausted");
        tab.tab = ViewerTab::new(
            id,
            tab.tab.label().into(),
            tab.tab.kind(),
            ViewerTabState::Error {
                reason: "render pending".into(),
            },
        );
        let generation = tab.generation;
        self.bump_revision();
        Some(ComputeTicket {
            tab_id: id,
            generation,
        })
    }

    pub(crate) fn publish_if_current(
        &mut self,
        ticket: ComputeTicket,
        value: CachedView,
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

        let label = tab_label::computed(&tab.recipe, &value.view);
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

    pub(crate) fn close(&mut self, id: ViewerTabId) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.tab.id() == id) else {
            return false;
        };
        self.tabs.remove(index);
        self.cache.remove(id);
        if self.active == Some(id) {
            self.active = self.tabs.last().map(|tab| tab.tab.id());
        }
        self.bump_revision();
        true
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
        self.close(id)
    }

    pub(crate) fn activate(&mut self, id: ViewerTabId) -> bool {
        if self.tabs.iter().any(|tab| tab.tab.id() == id) {
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

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    fn bump_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("viewer session revision exhausted");
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

    pub(crate) fn cached_fragment_if_current(
        &mut self,
        ticket: ComputeTicket,
        options: RenderOptions,
        theme: Theme,
    ) -> Option<std::sync::Arc<str>> {
        if self.current_ticket(ticket.tab_id) != Some(ticket) {
            return None;
        }
        self.cache
            .get(ticket.tab_id)
            .and_then(|cached| cached.fragments.get(&(options, theme)).cloned())
    }

    pub(crate) fn cache_fragment_if_current(
        &mut self,
        ticket: ComputeTicket,
        options: RenderOptions,
        theme: Theme,
        fragment: std::sync::Arc<str>,
    ) -> PublishOutcome {
        let current = self.current_ticket(ticket.tab_id) == Some(ticket)
            && self
                .tab(ticket.tab_id)
                .is_some_and(|tab| matches!(tab.tab.state(), ViewerTabState::Ready));
        if !current {
            return PublishOutcome::Stale;
        }
        self.cache
            .insert_fragment(ticket.tab_id, options, theme, fragment);
        PublishOutcome::Published
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use domain::{
        diffs::{Cmd, Commit, Foot, View},
        viewer::{ViewerTabId, ViewerTabKind},
    };
    use gtl_recipe::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

    use super::*;

    fn recipe() -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(PathBuf::from("/repo")),
            op: RecipeOp::SquashPreview { pinned: None },
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
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
            theme: None,
        })
    }

    fn view_with_context(
        repo_name: &str,
        branch: &str,
        upstream: &str,
        commit_count: usize,
    ) -> Arc<View> {
        let mut view = Arc::unwrap_or_clone(view("generic diff title"));
        view.repo_name = repo_name.into();
        view.branch = branch.into();
        view.upstream = upstream.into();
        view.commits = (0..commit_count)
            .map(|index| Commit {
                sha: format!("sha-{index}"),
                subject: format!("commit {index}"),
                ..Default::default()
            })
            .collect();
        Arc::new(view)
    }

    fn ready_session() -> (ViewerSession, ViewerTabId) {
        let mut session = ViewerSession::new(1024);
        let id = session.open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot);
        let ticket = session.begin_compute(id).expect("tab exists");
        assert_eq!(
            session.publish_if_current(ticket, CachedView::new(view("current"))),
            PublishOutcome::Published
        );
        (session, id)
    }

    #[test]
    fn reopening_a_recipe_reuses_its_tab_and_updates_the_batch() {
        let mut session = ViewerSession::new(128 * 1024 * 1024);
        let first = session.open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot);
        let second = session.open(recipe(), "batch-2".into(), ViewerTabKind::Snapshot);

        assert_eq!(first, second);
        assert_eq!(session.tab(first).expect("tab").batch_id, "batch-2");
    }

    #[test]
    fn reopening_a_recipe_updates_its_authoritative_kind() {
        let mut session = ViewerSession::new(1024);
        let id = session.open(recipe(), "snapshot".into(), ViewerTabKind::Snapshot);

        let reopened = session.open(recipe(), "live".into(), ViewerTabKind::Live);

        assert_eq!(reopened, id);
        let tab = session.tab(id).expect("tab");
        assert_eq!(tab.tab.kind(), ViewerTabKind::Live);
        assert_eq!(tab.batch_id, "live");
    }

    #[test]
    fn ready_tabs_use_concise_repository_and_operation_context() {
        let cases = [
            (
                Recipe {
                    source: RecipeSource::LocalRepo("/repos/project".into()),
                    op: RecipeOp::Diff {
                        target: RecipeTarget::Unpushed { pinned: None },
                    },
                    name: None,
                },
                view_with_context("project", "feature", "origin/main", 2),
                "project: 2 commits",
            ),
            (
                Recipe {
                    source: RecipeSource::LocalRepo("/repos/project".into()),
                    op: RecipeOp::MergeDiff {
                        base: Some("main".into()),
                        pinned: None,
                    },
                    name: None,
                },
                view_with_context("project", "feature", "main", 1),
                "project: merge feature->main",
            ),
            (
                Recipe {
                    source: RecipeSource::LocalRepo("/repos/project".into()),
                    op: RecipeOp::SquashPreview { pinned: None },
                    name: None,
                },
                view_with_context("project", "feature", "origin/main", 3),
                "project: squash 3 commits",
            ),
        ];

        for (recipe, view, expected) in cases {
            let mut session = ViewerSession::new(1024 * 1024);
            let id = session.open(recipe, "batch".into(), ViewerTabKind::Snapshot);
            let ticket = session.begin_compute(id).expect("tab exists");

            assert_eq!(
                session.publish_if_current(ticket, CachedView::new(view)),
                PublishOutcome::Published
            );
            assert_eq!(session.tab(id).expect("tab").tab.label(), expected);
        }
    }

    #[test]
    fn explicit_recipe_name_has_priority_over_computed_context() {
        let mut named = recipe();
        named.name = Some("Release review".into());
        let mut session = ViewerSession::new(1024 * 1024);
        let id = session.open(named, "batch".into(), ViewerTabKind::Snapshot);
        let ticket = session.begin_compute(id).expect("tab exists");

        session.publish_if_current(
            ticket,
            CachedView::new(view_with_context("project", "feature", "main", 4)),
        );

        assert_eq!(session.tab(id).expect("tab").tab.label(), "Release review");
    }

    #[test]
    fn unnamed_failures_preserve_the_recipe_specific_label() {
        let mut session = ViewerSession::new(1024);
        let id = session.open(recipe(), "batch".into(), ViewerTabKind::Snapshot);
        assert_eq!(session.tab(id).expect("tab").tab.label(), "repo: squash");
        let ticket = session.begin_compute(id).expect("tab exists");

        session.set_state_if_current(
            ticket,
            ViewerTabState::Error {
                reason: "safe failure".into(),
            },
        );

        assert_eq!(session.tab(id).expect("tab").tab.label(), "repo: squash");
    }

    #[test]
    fn blank_merge_target_failure_keeps_a_useful_generic_label() {
        let blank_merge = Recipe {
            source: RecipeSource::LocalRepo("/repos/project".into()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Merge {
                    base: "  ".into(),
                    pinned: None,
                },
            },
            name: None,
        };
        let mut session = ViewerSession::new(1024);
        let id = session.open(blank_merge, "batch".into(), ViewerTabKind::Snapshot);
        let ticket = session.begin_compute(id).expect("tab exists");

        session.set_state_if_current(
            ticket,
            ViewerTabState::Error {
                reason: "safe failure".into(),
            },
        );

        assert_eq!(session.tab(id).expect("tab").tab.label(), "project: merge");
    }

    #[test]
    fn stale_compute_cannot_overwrite_a_newer_refresh() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).expect("tab exists");
        let current = session.begin_compute(id).expect("tab exists");

        assert_eq!(
            session.publish_if_current(current, CachedView::new(view("newer"))),
            PublishOutcome::Published
        );
        assert_eq!(
            session.publish_if_current(stale, CachedView::new(view("stale"))),
            PublishOutcome::Stale
        );
        assert_eq!(
            session.cached_view(id).expect("current view").view.title,
            "newer"
        );
    }

    #[test]
    fn refresh_invalidates_cached_content_and_older_tickets() {
        let (mut session, id) = ready_session();
        let stale = session.begin_compute(id).expect("tab exists");

        let refresh = session.refresh(id).expect("tab exists");

        assert!(session.cached_view(id).is_none());
        assert_eq!(
            session.publish_if_current(stale, CachedView::new(view("stale"))),
            PublishOutcome::Stale
        );
        assert_eq!(
            session.publish_if_current(refresh, CachedView::new(view("fresh"))),
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
                reason: "render pending".into()
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
        let id = session.open(recipe(), "batch".into(), ViewerTabKind::Snapshot);
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
        assert!(session.close(id));
        assert!(session.revision() > failed);
    }

    #[test]
    fn close_invalidates_an_outstanding_compute() {
        let (mut session, id) = ready_session();
        let ticket = session.begin_compute(id).expect("tab exists");

        assert!(session.close(id));
        assert_eq!(
            session.publish_if_current(ticket, CachedView::new(view("late"))),
            PublishOutcome::Stale
        );
        assert!(session.cached_view(id).is_none());
    }

    #[test]
    fn activating_a_tab_updates_the_active_identity() {
        let mut session = ViewerSession::new(1024);
        let first = session.open(recipe(), "batch-1".into(), ViewerTabKind::Snapshot);
        let mut other = recipe();
        other.source = RecipeSource::LocalRepo(PathBuf::from("/other"));
        let second = session.open(other, "batch-1".into(), ViewerTabKind::Live);

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
                    pinned: Some(gtl_recipe::PinnedRange {
                        base: "a".repeat(40),
                        head: head.repeat(40),
                    }),
                },
            },
            name: None,
        }
    }

    #[test]
    fn open_dedupes_snapshot_tabs_by_unpinned_identity_and_adopts_the_new_pin() {
        let mut session = ViewerSession::new(1024 * 1024);
        let first = session.open(
            pinned_unpushed_recipe("b"),
            "batch-1".into(),
            ViewerTabKind::Snapshot,
        );
        let second = session.open(
            pinned_unpushed_recipe("c"),
            "batch-2".into(),
            ViewerTabKind::Snapshot,
        );

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
        let a = session.open(
            range_recipe("a..b"),
            "batch-1".into(),
            ViewerTabKind::Snapshot,
        );
        let b = session.open(
            range_recipe("c..d"),
            "batch-2".into(),
            ViewerTabKind::Snapshot,
        );
        assert_ne!(a, b);
    }
}
