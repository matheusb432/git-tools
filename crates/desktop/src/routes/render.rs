use std::sync::{Arc, Mutex};

use application::{
    diffs::View,
    viewer::{ViewerDocument, ViewerHistoryEntry, ViewerSettings, ViewerTabState, ViewerView},
};
use maud::{DOCTYPE, html};

use crate::{
    render::{MaudViewerRenderer, SwapFeedback},
    session::{ComputeTicket, ViewerSession},
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

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
pub(super) fn view_with_tabs(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    mut transient: Option<VersionedView>,
    settings: ViewerSettings,
) -> Result<String, RenderError> {
    for _ in 0..MAX_GENERATION_RETRIES {
        let snapshot = match snapshot(session, transient.take(), Vec::new(), settings.clone()) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let Some(ticket) = snapshot.ticket else {
            let html = renderer.build_view_with_tabs(&snapshot.document);
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
        let html = renderer.build_view_with_tabs(&snapshot.document);
        if snapshot_is_current(session, Some(ticket), snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
pub(super) fn tabs_with_view(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    mut transient: Option<VersionedView>,
    settings: ViewerSettings,
    feedback: SwapFeedback<'_>,
) -> Result<String, RenderError> {
    for _ in 0..MAX_GENERATION_RETRIES {
        let snapshot = match snapshot(session, transient.take(), Vec::new(), settings.clone()) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let html = renderer.build_tabs_with_view(&snapshot.document, feedback);
        if snapshot_is_current(session, snapshot.ticket, snapshot.revision)? {
            return Ok(html);
        }
    }
    Err(RenderError::Conflict)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "bounded retries clone one validated settings value"
)]
pub(super) fn tabs_only(
    renderer: MaudViewerRenderer,
    session: &Mutex<ViewerSession>,
    settings: ViewerSettings,
    feedback: SwapFeedback<'_>,
) -> Result<String, RenderError> {
    for _ in 0..MAX_GENERATION_RETRIES {
        let snapshot = match snapshot(session, None, Vec::new(), settings.clone()) {
            Ok(snapshot) => snapshot,
            Err(RenderError::Retry) => continue,
            Err(error) => return Err(error),
        };
        let html = renderer.build_tabs_only(&snapshot.document, feedback);
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
        viewer::{RenderOptions, Theme, ViewerSettings, ViewerTabKind},
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

    fn settings() -> ViewerSettings {
        ViewerSettings::new(RenderOptions::DEFAULT, Theme::Dark)
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

        let error = view_with_tabs(
            MaudViewerRenderer,
            &session,
            Some(VersionedView {
                ticket: old,
                view: old_view,
            }),
            settings(),
        )
        .expect_err("ready oversize view retries to conflict");
        assert!(matches!(error, RenderError::Conflict));
        let mut state = session.lock().expect("session");
        assert_eq!(state.current_ticket(id), Some(current));
        assert!(state.cached_view_snapshot(id).is_none());
    }
}
