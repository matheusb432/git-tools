//! The active diff view: the ready view (controls + shared diff body) and the
//! empty, broken, and render-failed states with their recovery actions.

use gtl_application::viewer::{ViewerDocument, ViewerTab, ViewerTabKind, ViewerTabState};
use maud::{Markup, html};

use super::{SwapFeedback, SwapMode, controls};
use crate::{
    materialization::ViewLoadId,
    render::{
        VIEW_STATE_BROKEN, VIEW_STATE_EMPTY, VIEW_STATE_ERROR, VIEW_STATE_LOADING,
        VIEW_STATE_READY, ViewerRoute,
    },
    session::RENDER_PENDING_REASON,
};

const STATUS_LAYOUT_CLASSES: &str = "grid min-h-full grid-cols-[auto_minmax(0,520px)] place-content-center gap-[18px] p-8 text-ink-2";
const RETRY_BUTTON_CLASSES: &str = "viewer-recovery-button mt-4 cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-[11px] py-[7px] text-xs text-acc [font:inherit] hover:bg-acc hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-line-2 [&.htmx-request]:bg-surface-2 [&.htmx-request]:text-ink";
const SHOW_ALL_BUTTON_CLASSES: &str = "viewer-recovery-button mt-4 cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-[11px] py-[7px] text-xs text-acc [font:inherit] hover:bg-acc hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";

pub(in crate::render) fn view(
    document: &ViewerDocument,
    swap: SwapMode,
    feedback: SwapFeedback<'_>,
    load_id: Option<ViewLoadId>,
    defer_ready: bool,
) -> Markup {
    let selection_pending = document
        .active_view()
        .is_some_and(gtl_application::viewer::ViewerView::selection_is_pending);
    let state = match document.active_tab().map(ViewerTab::state) {
        None => VIEW_STATE_EMPTY,
        Some(ViewerTabState::Ready) if defer_ready || selection_pending => VIEW_STATE_LOADING,
        Some(ViewerTabState::Ready) => VIEW_STATE_READY,
        Some(ViewerTabState::Broken { .. }) => VIEW_STATE_BROKEN,
        Some(ViewerTabState::Error { reason }) if reason == RENDER_PENDING_REASON => {
            VIEW_STATE_LOADING
        }
        Some(ViewerTabState::Error { .. }) => VIEW_STATE_ERROR,
    };
    let view_identity = document.active_tab_id().map(|tab_id| {
        document.active_view().map_or_else(
            || format!("{tab_id}:range"),
            |view| {
                view.selected_commit_sha().map_or_else(
                    || format!("{tab_id}:range"),
                    |sha| format!("{tab_id}:commit:{sha}"),
                )
            },
        )
    });

    html! {
        section id="viewer-view"
            hx-swap-oob=[swap.out_of_band()]
            class=(if state == VIEW_STATE_READY {
                "viewer-view grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft [&>.layout]:h-full [&>.layout]:min-h-0"
            } else {
                "viewer-view min-h-0 min-w-0 overflow-hidden [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft"
            })
            data-viewer-state=(state)
            data-view-identity=[view_identity]
            data-tab-id=[document.active_tab_id().map(|id| id.to_string())] {
            @match document.active_tab() {
                None => (empty_view(matches!(
                    feedback,
                    SwapFeedback::TabClosed | SwapFeedback::LiveViewDeleted
                ))),
                Some(tab) => @match tab.state() {
                    ViewerTabState::Ready => {
                        @if !defer_ready && !selection_pending {
                            @let view = document.active_view().expect("ViewerDocument guarantees a view for the ready active tab");
                            @if let Some(reason) = view.selection_error_reason() {
                                (selected_commit_error(view, reason))
                            } @else {
                                (controls::view_controls(view))
                                @let mobile_controls = controls::mobile_view_controls(view, document.settings().theme());
                                @if let Some(load_id) = load_id {
                                    (gtl_preview::view_shell_with_mobile_controls(
                                        view.view(),
                                        view.range_view(),
                                        view.selected_commit_sha(),
                                        view.options(),
                                        view.tab_id(),
                                        load_id.get(),
                                        mobile_controls,
                                    ))
                                } @else {
                                    (gtl_preview::view_fragment_with_mobile_controls(
                                        view.view(),
                                        view.range_view(),
                                        view.selected_commit_sha(),
                                        view.options(),
                                        view.tab_id(),
                                        mobile_controls,
                                    ))
                                }
                            }
                        }
                    },
                    ViewerTabState::Broken { code, reason } => (failure_view(tab, TabFailure::Broken { code, reason })),
                    ViewerTabState::Error { reason } if reason == RENDER_PENDING_REASON => {},
                    ViewerTabState::Error { reason } => (failure_view(tab, TabFailure::Render { reason })),
                }
            }
        }
    }
}

pub(in crate::render) fn loading_template() -> Markup {
    html! {
        template id="viewer-loading-template" {
            section id="viewer-view"
                class="viewer-view min-h-0 min-w-0 overflow-hidden"
                data-viewer-state=(VIEW_STATE_LOADING)
                aria-busy="true" {
                div class="grid h-full min-h-0 grid-rows-[44px_minmax(0,1fr)] bg-bg" role="status" aria-label="Loading diff" {
                    div class="animate-pulse border-b border-line bg-surface-2 px-4 py-3" {
                        div class="h-4 w-2/5 rounded-sm bg-line-2" {}
                    }
                    div class="grid min-h-0 grid-cols-[minmax(180px,22%)_minmax(0,1fr)_minmax(180px,20%)] gap-4 p-4 mobile:grid-cols-1" {
                        div class="animate-pulse rounded-panel border border-line bg-surface p-3" {
                            div class="mb-3 h-3 w-3/4 rounded-sm bg-line-2" {}
                            div class="mb-2 h-3 w-full rounded-sm bg-line" {}
                            div class="mb-2 h-3 w-5/6 rounded-sm bg-line" {}
                            div class="h-3 w-2/3 rounded-sm bg-line" {}
                        }
                        div class="animate-pulse rounded-panel border border-line bg-surface p-3" {
                            @for width in ["w-5/6", "w-full", "w-4/5", "w-11/12", "w-3/4", "w-full", "w-4/5"] {
                                div class={ "mb-2 h-3 rounded-sm bg-line " (width) } {}
                            }
                        }
                        div class="animate-pulse rounded-panel border border-line bg-surface p-3 mobile:hidden" {
                            div class="mb-3 h-3 w-2/3 rounded-sm bg-line-2" {}
                            div class="mb-2 h-8 w-full rounded-sm bg-line" {}
                            div class="mb-2 h-8 w-full rounded-sm bg-line" {}
                            div class="h-8 w-full rounded-sm bg-line" {}
                        }
                    }
                }
            }
        }
    }
}

fn selected_commit_error(view: &gtl_application::viewer::ViewerView, reason: &str) -> Markup {
    html! {
        div class="viewer-status viewer-status-error grid min-h-full grid-cols-[auto_minmax(0,520px)] place-content-center gap-[18px] p-8 text-ink-2" role="alert" {
            span class="viewer-status-mark flex size-[42px] items-center justify-center rounded-full border border-del-line bg-del-bg text-xl font-bold text-del" aria-hidden="true" { "×" }
            div {
                p class="viewer-status-eyebrow m-0 text-[10px] font-bold tracking-[.08em] text-ink-3 uppercase" { "Commit patch unavailable" }
                h1 class="mt-[3px] mb-[7px] text-xl leading-tight tracking-[-.02em] text-ink" { "The selected commit could not be rendered" }
                p class="m-0 [overflow-wrap:anywhere]" { (reason) }
                (controls::show_all_changes_button(view, SHOW_ALL_BUTTON_CLASSES))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TabFailure<'a> {
    Broken { code: &'a str, reason: &'a str },
    Render { reason: &'a str },
}

fn failure_view(tab: &ViewerTab, failure: TabFailure<'_>) -> Markup {
    let (status_class, role, mark_classes, mark, title, action) = match failure {
        TabFailure::Broken { .. } => (
            "viewer-status-broken",
            "status",
            "border-line-2 bg-surface text-acc",
            "!",
            "This diff cannot be opened",
            "Try again",
        ),
        TabFailure::Render { .. } => (
            "viewer-status-error",
            "alert",
            "border-del-line bg-del-bg text-del",
            "×",
            "The diff could not be rendered",
            "Render again",
        ),
    };
    let reason = match failure {
        TabFailure::Broken { reason, .. } | TabFailure::Render { reason } => reason,
    };

    html! {
        div class={ "viewer-status " (status_class) " " (STATUS_LAYOUT_CLASSES) } role=(role) {
            span class={ "viewer-status-mark flex size-[42px] items-center justify-center rounded-full border text-xl font-bold " (mark_classes) } aria-hidden="true" { (mark) }
            div {
                p class="viewer-status-eyebrow m-0 text-[10px] font-bold tracking-[.08em] text-ink-3 uppercase" {
                    @match failure {
                        TabFailure::Broken { code, .. } => {
                            "Unavailable · " code class="rounded-sm border border-line bg-sunk px-[5px] py-px text-ink [font:inherit]" { (code) }
                        }
                        TabFailure::Render { .. } => { "Render failed" }
                    }
                }
                h1 class="mt-[3px] mb-[7px] text-xl leading-tight tracking-[-.02em] text-ink" { (title) }
                p class="m-0 [overflow-wrap:anywhere]" { (reason) }
                button type="button"
                    class=(RETRY_BUTTON_CLASSES)
                    hx-get=(ViewerRoute::Refresh { tab: tab.id() })
                    hx-target="#viewer-view"
                    hx-sync="#viewer-view:replace"
                    hx-swap="outerHTML" { (action) }
                @if tab.kind() == ViewerTabKind::Live {
                    (controls::delete_live_view_button(tab.id()))
                }
            }
        }
    }
}

fn empty_view(focus_history: bool) -> Markup {
    html! {
        div class="viewer-status viewer-status-empty grid min-h-full grid-cols-[auto_minmax(0,520px)] place-content-center gap-[18px] p-8 text-ink-2" {
            span class="viewer-status-mark flex size-[42px] items-center justify-center rounded-full border border-line-2 bg-surface text-xl font-bold text-acc" aria-hidden="true" { "±" }
            div {
                p class="viewer-status-eyebrow m-0 text-[10px] font-bold tracking-[.08em] text-ink-3 uppercase" { "Viewer ready" }
                h1 class="mt-[3px] mb-[7px] text-xl leading-tight tracking-[-.02em] text-ink" { "No diff open" }
                p class="m-0 [overflow-wrap:anywhere]" { "Run " code class="rounded-sm border border-line bg-sunk px-[5px] py-px text-ink [font:inherit]" { "gtl diff" } " in a repository, or choose a previous render from History." }
                button type="button" class="viewer-recovery-button mt-4 cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-[11px] py-[7px] text-xs text-acc [font:inherit] hover:bg-acc hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc" autofocus[focus_history] popovertarget="viewer-history-popover" { "Open history" }
            }
        }
    }
}
