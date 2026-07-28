//! The active diff view: the ready view (controls + shared diff body) and the
//! empty, broken, and render-failed states with their recovery actions.

use application::viewer::{ViewerDocument, ViewerTab, ViewerTabKind, ViewerTabState};
use maud::{Markup, html};

use super::{SwapFeedback, SwapMode, controls};
use crate::render::ViewerRoute;

pub(in crate::render) fn view(
    document: &ViewerDocument,
    swap: SwapMode,
    feedback: SwapFeedback<'_>,
) -> Markup {
    let state = match document.active_tab().map(ViewerTab::state) {
        None => "empty",
        Some(ViewerTabState::Ready) => "ready",
        Some(ViewerTabState::Broken { .. }) => "broken",
        Some(ViewerTabState::Error { .. }) => "error",
    };

    html! {
        section id="viewer-view"
            hx-swap-oob=[swap.out_of_band()]
            class=(if state == "ready" {
                "viewer-view grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft [&>.layout]:h-full [&>.layout]:min-h-0"
            } else {
                "viewer-view min-h-0 min-w-0 overflow-hidden [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft"
            })
            data-viewer-state=(state)
            data-tab-id=[document.active_tab_id().map(|id| id.to_string())] {
            @match document.active_tab() {
                None => (empty_view(feedback == SwapFeedback::LiveViewDeleted)),
                Some(tab) => @match tab.state() {
                    ViewerTabState::Ready => {
                        @let view = document.active_view().expect("ViewerDocument guarantees a view for the ready active tab");
                        (controls::view_controls(view, document.settings().theme()))
                        (preview::view_fragment(view.view(), view.options(), view.tab_id()))
                    },
                    ViewerTabState::Broken { code, reason } => (broken_view(tab, code, reason)),
                    ViewerTabState::Error { reason } => (error_view(tab, reason)),
                }
            }
        }
    }
}

fn broken_view(tab: &ViewerTab, code: &str, reason: &str) -> Markup {
    html! {
        div class="viewer-status viewer-status-broken grid min-h-full grid-cols-[auto_minmax(0,520px)] place-content-center gap-[18px] p-8 text-ink-2" role="status" {
            span class="viewer-status-mark flex size-[42px] items-center justify-center rounded-full border border-line-2 bg-surface text-xl font-bold text-acc" aria-hidden="true" { "!" }
            div {
                p class="viewer-status-eyebrow m-0 text-[10px] font-bold tracking-[.08em] text-ink-3 uppercase" { "Unavailable · " code class="rounded-sm border border-line bg-sunk px-[5px] py-px text-ink [font:inherit]" { (code) } }
                h1 class="mt-[3px] mb-[7px] text-xl leading-tight tracking-[-.02em] text-ink" { "This diff cannot be opened" }
                p class="m-0 [overflow-wrap:anywhere]" { (reason) }
                button type="button"
                    class="viewer-recovery-button mt-4 cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-[11px] py-[7px] text-xs text-acc [font:inherit] hover:bg-acc hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-line-2 [&.htmx-request]:bg-surface-2 [&.htmx-request]:text-ink"
                    hx-get=(ViewerRoute::Refresh { tab: tab.id() })
                    hx-target="#viewer-view"
                    hx-swap="outerHTML" { "Try again" }
                @if tab.kind() == ViewerTabKind::Live {
                    (controls::delete_live_view_button(tab.id()))
                }
            }
        }
    }
}

fn error_view(tab: &ViewerTab, reason: &str) -> Markup {
    html! {
        div class="viewer-status viewer-status-error grid min-h-full grid-cols-[auto_minmax(0,520px)] place-content-center gap-[18px] p-8 text-ink-2" role="alert" {
            span class="viewer-status-mark flex size-[42px] items-center justify-center rounded-full border border-del-line bg-del-bg text-xl font-bold text-del" aria-hidden="true" { "×" }
            div {
                p class="viewer-status-eyebrow m-0 text-[10px] font-bold tracking-[.08em] text-ink-3 uppercase" { "Render failed" }
                h1 class="mt-[3px] mb-[7px] text-xl leading-tight tracking-[-.02em] text-ink" { "The diff could not be rendered" }
                p class="m-0 [overflow-wrap:anywhere]" { (reason) }
                button type="button"
                    class="viewer-recovery-button mt-4 cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-[11px] py-[7px] text-xs text-acc [font:inherit] hover:bg-acc hover:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-line-2 [&.htmx-request]:bg-surface-2 [&.htmx-request]:text-ink"
                    hx-get=(ViewerRoute::Refresh { tab: tab.id() })
                    hx-target="#viewer-view"
                    hx-swap="outerHTML" { "Render again" }
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
