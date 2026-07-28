use std::sync::{Arc, Mutex};

use application::{
    diffs::View,
    viewer::{ViewerDocument, ViewerHistoryEntry, ViewerSettings, ViewerTabState, ViewerView},
};
use maud::{DOCTYPE, html};

use crate::{
    render::MaudViewerRenderer,
    session::{ComputeTicket, PublishOutcome, ViewerSession},
};

const MAX_GENERATION_RETRIES: usize = 3;

#[derive(Debug, Clone)]
pub(super) struct VersionedView {
    pub(super) ticket: ComputeTicket,
    pub(super) view: Arc<View>,
}

#[derive(Debug)]
pub(super) enum RenderError {
    State(String),
    Retry,
    Conflict,
}

impl From<String> for RenderError {
    fn from(value: String) -> Self {
        Self::State(value)
    }
}

struct RouteSnapshot {
    document: ViewerDocument,
    ticket: Option<ComputeTicket>,
    revision: u64,
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one gathered document snapshot"
)]
pub(super) fn document(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    history: Vec<ViewerHistoryEntry>,
    settings: ViewerSettings,
) -> Result<String, RenderError> {
    let mut transient = transient;
    for _ in 0..MAX_GENERATION_RETRIES {
        let snapshot = match snapshot(session, transient.take(), history.clone(), settings.clone())
        {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let html = renderer.build_document(&snapshot.document);
        if snapshot_is_current(session, snapshot.ticket, snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}

pub(super) fn view_with_tabs(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    settings: ViewerSettings,
) -> Result<String, RenderError> {
    view_with_tabs_using(renderer, session, transient, settings, || {})
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
fn view_with_tabs_using(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    mut transient: Option<VersionedView>,
    settings: ViewerSettings,
    mut after_render: impl FnMut(),
) -> Result<String, RenderError> {
    for _ in 0..MAX_GENERATION_RETRIES {
        let snapshot = match snapshot(session, transient.take(), Vec::new(), settings.clone()) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let Some(ticket) = snapshot.ticket else {
            let html = renderer.build_view_with_tabs(&snapshot.document);
            after_render();
            if snapshot_is_current(session, None, snapshot.revision)? {
                return Ok(html);
            }
            continue;
        };
        if !matches!(
            snapshot
                .document
                .active_tab()
                .map(application::viewer::ViewerTab::state),
            Some(ViewerTabState::Ready)
        ) {
            let html = renderer.build_view_with_tabs(&snapshot.document);
            if snapshot_is_current(session, Some(ticket), snapshot.revision)? {
                return Ok(html);
            }
            continue;
        }
        let options = settings.options();
        let cached = {
            let mut session = session
                .lock()
                .map_err(|_| RenderError::State("session lock poisoned".into()))?;
            session.cached_fragment_if_current(ticket, options)
        };
        if let Some(cached) = cached {
            let html = renderer.build_cached_view_with_tabs(&cached, &snapshot.document);
            if snapshot_is_current(session, Some(ticket), snapshot.revision)? {
                return Ok(html);
            }
            continue;
        }
        let fragment: Arc<str> = Arc::from(renderer.build_view(&snapshot.document));
        after_render();
        let published = session
            .lock()
            .map_err(|_| RenderError::State("session lock poisoned".into()))?
            .cache_fragment_if_current(ticket, options, Arc::clone(&fragment));
        if published == PublishOutcome::Published {
            let html = renderer.build_cached_view_with_tabs(&fragment, &snapshot.document);
            if snapshot_is_current(session, Some(ticket), snapshot.revision)? {
                return Ok(html);
            }
        }
    }
    Err(RenderError::Conflict)
}

pub(super) fn tabs_with_view(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    settings: ViewerSettings,
) -> Result<String, RenderError> {
    tabs_with_view_using(
        renderer,
        session,
        transient,
        settings,
        MaudViewerRenderer::build_tabs_with_view,
    )
}

pub(super) fn tabs_with_view_after_live_delete(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    settings: ViewerSettings,
) -> Result<String, RenderError> {
    tabs_with_view_using(
        renderer,
        session,
        transient,
        settings,
        MaudViewerRenderer::build_tabs_with_view_after_live_delete,
    )
}

pub(super) fn tabs_with_view_after_snapshot_skips(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    settings: ViewerSettings,
    skipped_labels: &[String],
) -> Result<String, RenderError> {
    tabs_with_view_using(
        renderer,
        session,
        transient,
        settings,
        |renderer, document| {
            renderer.build_tabs_with_view_after_snapshot_skips(document, skipped_labels)
        },
    )
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
fn tabs_with_view_using(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    mut transient: Option<VersionedView>,
    settings: ViewerSettings,
    build: impl Fn(MaudViewerRenderer, &ViewerDocument) -> String,
) -> Result<String, RenderError> {
    for _ in 0..MAX_GENERATION_RETRIES {
        let snapshot = match snapshot(session, transient.take(), Vec::new(), settings.clone()) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let html = build(renderer, &snapshot.document);
        if snapshot_is_current(session, snapshot.ticket, snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}

fn snapshot_is_current(
    session: &Mutex<ViewerSession>,
    ticket: Option<ComputeTicket>,
    revision: u64,
) -> Result<bool, RenderError> {
    let session = session
        .lock()
        .map_err(|_| RenderError::State("session lock poisoned".into()))?;
    Ok(session.revision() == revision
        && ticket.is_none_or(|ticket| session.current_ticket(ticket.tab_id) == Some(ticket)))
}

pub(super) fn history(renderer: MaudViewerRenderer, entries: &[ViewerHistoryEntry]) -> String {
    renderer.build_history(entries)
}

pub(super) fn error_document() -> String {
    html! {
        (DOCTYPE)
        html lang="en" { head { meta charset="utf-8"; title { "git-tools viewer error" } }
            body { main role="alert" { h1 { "The viewer could not complete this request" } p { "Please retry the operation." } } }
        }
    }.into_string()
}

pub(super) fn error_view() -> String {
    html! {
        section id="viewer-view" class="viewer-view min-h-0 min-w-0 overflow-hidden [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft" data-viewer-state="error" {
            div class="viewer-status grid min-h-full grid-cols-[minmax(0,520px)] place-content-center p-8 text-ink-2" role="alert" {
                strong class="text-ink" { "The view could not be updated" }
                p class="mt-1 mb-0" { "Please retry the operation." }
            }
        }
    }
    .into_string()
}

pub(super) fn error_tabs() -> String {
    html! {
        nav id="viewer-tabs" class="viewer-tabs z-[70] flex min-w-0 items-end gap-2.5 border-b border-line bg-surface px-3 pt-2 [&.htmx-swapping]:border-acc-line [&.htmx-settling]:border-acc-line [@media(max-width:760px)]:px-2" aria-label="Open diffs" {
            div class="mb-2 rounded-sm border border-del-line bg-del-bg px-2.5 py-1.5 text-xs text-del" role="alert" {
                strong { "The tabs could not be updated" }
                span { " Please retry the operation." }
            }
        }
    }
    .into_string()
}

pub(super) fn error_history() -> String {
    html! {
        section id="viewer-history" class="viewer-history gtl-scroll h-[calc(100%-58px)] overflow-auto px-4 py-3.5 [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft" {
            div class="rounded-sm border border-del-line bg-del-bg px-2.5 py-2 text-xs text-del" role="alert" {
                "History could not be loaded. Please retry."
            }
        }
    }
    .into_string()
}

fn snapshot(
    session: &Mutex<ViewerSession>,
    transient: Option<VersionedView>,
    history: Vec<ViewerHistoryEntry>,
    settings: ViewerSettings,
) -> Result<RouteSnapshot, RenderError> {
    let mut session = session
        .lock()
        .map_err(|_| RenderError::State("session lock poisoned".into()))?;
    let tabs = session
        .tabs()
        .map(|entry| entry.tab.clone())
        .collect::<Vec<_>>();
    let active = session.active();
    let ticket = active.and_then(|id| session.current_ticket(id));
    let revision = session.revision();
    let active_view = active.and_then(|id| {
        let kind = session.tab(id)?.tab.kind();
        let view = transient
            .filter(|value| Some(value.ticket) == ticket && value.ticket.tab_id == id)
            .map(|value| value.view)
            .or_else(|| session.cached_view_snapshot(id).map(|cached| cached.view))?;
        Some(ViewerView::new(id, view, settings.options(), kind))
    });
    drop(session);
    let document =
        ViewerDocument::new(tabs, active, active_view, history, settings).map_err(|error| {
            if matches!(
                error,
                application::viewer::ViewerDocumentError::ReadyTabMissingView { .. }
            ) {
                RenderError::Retry
            } else {
                RenderError::State(error.to_string())
            }
        })?;
    Ok(RouteSnapshot {
        document,
        ticket,
        revision,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use application::{
        diffs::{Cmd, Foot, View},
        viewer::{
            RenderOptions, Theme, ViewerSettings, ViewerTabId, ViewerTabKind, ViewerTabState,
        },
    };
    use contracts::recipes::{Recipe, RecipeOp, RecipeSource};

    use super::*;
    use crate::session::{CachedView, ViewerSession};

    fn view(title: &str) -> Arc<View> {
        Arc::new(View {
            exclusions: None,
            repo_name: "repo".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "main".into(),
            commits: vec![],
            files: vec![],
            title: title.into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
        })
    }

    fn ready_session() -> (Mutex<ViewerSession>, ViewerTabId) {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo("/repo".into()),
            op: RecipeOp::SquashPreview { pinned: None },
            name: None,
        };
        let mut session = ViewerSession::new(1024 * 1024);
        let id = session.open(recipe, "batch".into(), ViewerTabKind::Snapshot);
        let ticket = session.begin_compute(id).expect("ticket");
        session.publish_labeled_if_current(
            ticket,
            CachedView::new(view("stale secret")),
            "ready".into(),
        );
        (Mutex::new(session), id)
    }

    fn settings() -> ViewerSettings {
        ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)
    }

    #[test]
    fn error_publication_between_render_and_cache_cannot_leak_stale_html() {
        let (session, id) = ready_session();
        let mut interleaved = false;
        let html = view_with_tabs_using(MaudViewerRenderer, &session, None, settings(), || {
            if interleaved {
                return;
            }
            interleaved = true;
            let mut session = session.lock().expect("session lock");
            let ticket = session.refresh(id).expect("refresh ticket");
            session.set_state_if_current(
                ticket,
                ViewerTabState::Error {
                    reason: "current failure".into(),
                },
            );
        })
        .expect("fresh error snapshot renders");

        assert!(html.contains("current failure"));
        assert!(!html.contains("aria-label=\"Diff display controls\""));
        assert!(!html.contains("id=\"viewer-controls-popover\""));
        let mut session = session.lock().expect("session lock");
        let current = session.current_ticket(id).expect("ticket");
        assert!(
            session
                .cached_fragment_if_current(current, RenderOptions::DEFAULT)
                .is_none()
        );
    }

    #[test]
    fn repeated_refresh_interleaving_returns_conflict_without_caching_old_html() {
        let (session, id) = ready_session();
        let mut generation = 0;
        let error = view_with_tabs_using(MaudViewerRenderer, &session, None, settings(), || {
            generation += 1;
            let mut session = session.lock().expect("session lock");
            let ticket = session.refresh(id).expect("refresh ticket");
            session.publish_labeled_if_current(
                ticket,
                CachedView::new(view(&format!("fresh {generation}"))),
                "ready".into(),
            );
        })
        .expect_err("continuous refresh exhausts bounded retry");

        assert!(matches!(error, RenderError::Conflict));
        let mut session = session.lock().expect("session lock");
        let current = session.current_ticket(id).expect("ticket");
        assert!(
            session
                .cached_fragment_if_current(current, RenderOptions::DEFAULT)
                .is_none()
        );
    }

    #[test]
    fn tab_open_during_empty_composition_never_returns_old_empty_html() {
        let session = Mutex::new(ViewerSession::new(1024));
        let mut opened = false;
        let html = view_with_tabs_using(MaudViewerRenderer, &session, None, settings(), || {
            if opened {
                return;
            }
            opened = true;
            session.lock().expect("session").open(
                Recipe {
                    source: RecipeSource::LocalRepo("/new".into()),
                    op: RecipeOp::SquashPreview { pinned: None },
                    name: None,
                },
                "new".into(),
                ViewerTabKind::Snapshot,
            );
        })
        .expect("fresh snapshot renders");
        assert!(!html.contains("No diff open"));
        assert!(html.contains("data-tab-id=\"1\""));
    }

    #[test]
    fn active_change_and_close_during_composition_return_fresh_shell() {
        let (session, first) = ready_session();
        let second = {
            let mut state = session.lock().expect("session");
            let id = state.open(
                Recipe {
                    source: RecipeSource::LocalRepo("/two".into()),
                    op: RecipeOp::SquashPreview { pinned: None },
                    name: None,
                },
                "two".into(),
                ViewerTabKind::Snapshot,
            );
            let ticket = state.begin_compute(id).expect("ticket");
            state.publish_labeled_if_current(
                ticket,
                CachedView::new(view("second")),
                "ready".into(),
            );
            state.activate(first);
            id
        };
        let mut changed = false;
        let html = view_with_tabs_using(MaudViewerRenderer, &session, None, settings(), || {
            if changed {
                return;
            }
            changed = true;
            let mut state = session.lock().expect("session");
            state.activate(second);
            state.close(first);
        })
        .expect("fresh shell renders");
        assert!(html.contains(&format!("data-tab-id=\"{second}\"")));
        assert!(!html.contains("data-tab-id=\"1\""));
    }

    #[test]
    fn inactive_tab_publication_during_composition_refreshes_tab_markup() {
        let (session, first) = ready_session();
        let second = {
            let mut state = session.lock().expect("session");
            let id = state.open(
                Recipe {
                    source: RecipeSource::LocalRepo("/two".into()),
                    op: RecipeOp::SquashPreview { pinned: None },
                    name: None,
                },
                "two".into(),
                ViewerTabKind::Snapshot,
            );
            let ticket = state.begin_compute(id).expect("ticket");
            state.publish_labeled_if_current(
                ticket,
                CachedView::new(view("second")),
                "ready".into(),
            );
            state.activate(first);
            id
        };
        let mut changed = false;
        let html = view_with_tabs_using(MaudViewerRenderer, &session, None, settings(), || {
            if changed {
                return;
            }
            changed = true;
            let mut state = session.lock().expect("session");
            let ticket = state.begin_compute(second).expect("ticket");
            state.set_state_if_current(
                ticket,
                ViewerTabState::Error {
                    reason: "safe".into(),
                },
            );
        })
        .expect("fresh shell renders");
        assert!(html.contains("aria-label=\"Render failed\""));
        assert!(html.contains(&format!("data-tab-id=\"{first}\"")));
    }

    #[test]
    fn cached_view_fragment_survives_a_settings_theme_change() {
        let (session, _id) = ready_session();
        let mut renders = 0;
        let dark_settings = ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark);
        let first_html =
            view_with_tabs_using(MaudViewerRenderer, &session, None, dark_settings, || {
                renders += 1;
            })
            .expect("first render populates the fragment cache");
        assert_eq!(renders, 1, "cache miss renders the fragment once");
        assert!(first_html.contains("id=\"viewer-theme-name\">Dark<"));

        let light_settings = ViewerSettings::new(RenderOptions::DEFAULT, Theme::Light);
        let second_html =
            view_with_tabs_using(MaudViewerRenderer, &session, None, light_settings, || {
                renders += 1;
            })
            .expect("second render reuses the cached fragment");

        // The tab strip is rendered fresh every time and legitimately reflects
        // the new theme; only the swapped `#viewer-view` fragment must survive
        // the theme change unchanged.
        assert_eq!(
            renders, 1,
            "the settings theme changing must not force a fresh render: \
             the cached fragment is served instead"
        );
        assert!(second_html.contains("id=\"viewer-theme-name\">Light<"));
        let (first_view, _) = first_html
            .split_once("<nav id=\"viewer-tabs\"")
            .expect("the view fragment precedes the tab strip");
        let (second_view, _) = second_html
            .split_once("<nav id=\"viewer-tabs\"")
            .expect("the view fragment precedes the tab strip");
        assert_eq!(
            first_view, second_view,
            "the cached view bytes are served verbatim across the theme change"
        );
    }

    #[test]
    fn stale_transient_followed_by_current_oversize_view_is_bounded_conflict() {
        let recipe = Recipe {
            source: RecipeSource::LocalRepo("/repo".into()),
            op: RecipeOp::SquashPreview { pinned: None },
            name: None,
        };
        let mut state = ViewerSession::new(1);
        let id = state.open(recipe, "batch".into(), ViewerTabKind::Snapshot);
        let old = state.begin_compute(id).expect("old ticket");
        let old_view = view("old transient");
        state.publish_labeled_if_current(old, CachedView::new(Arc::clone(&old_view)), "old".into());
        let current = state.begin_compute(id).expect("current ticket");
        state.publish_labeled_if_current(
            current,
            CachedView::new(view("current oversize")),
            "current".into(),
        );
        let session = Mutex::new(state);

        let error = view_with_tabs_using(
            MaudViewerRenderer,
            &session,
            Some(VersionedView {
                ticket: old,
                view: old_view,
            }),
            settings(),
            || {},
        )
        .expect_err("ready oversize view retries to conflict");
        assert!(matches!(error, RenderError::Conflict));
        let mut state = session.lock().expect("session");
        assert_eq!(state.current_ticket(id), Some(current));
        assert!(state.cached_view_snapshot(id).is_none());
        assert!(
            state
                .cached_fragment_if_current(current, RenderOptions::DEFAULT)
                .is_none()
        );
    }
}
