//! The tab strip: one tab per open diff, the History button, and the
//! accessible feedback (skip toast, deletion announcement) a compound
//! response attaches to it.

use application::viewer::{ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
use maud::{Markup, html};

use super::{SwapFeedback, SwapMode};
use crate::render::ViewerRoute;

pub(in crate::render) fn tabs(
    tabs: &[ViewerTab],
    active_tab_id: Option<ViewerTabId>,
    swap: SwapMode,
    feedback: SwapFeedback<'_>,
) -> Markup {
    html! {
        nav id="viewer-tabs" hx-swap-oob=[swap.out_of_band()] class="viewer-tabs z-[70] flex min-w-0 items-end gap-2.5 border-b border-line bg-surface px-3 pt-2 [&.htmx-swapping]:border-acc-line [&.htmx-settling]:border-acc-line [@media(max-width:760px)]:px-2" aria-label="Open diffs" {
            ul class="viewer-tab-list m-0 flex min-w-0 flex-1 list-none items-end gap-1 overflow-x-auto p-0 [scrollbar-width:thin]" {
                @for tab in tabs {
                    @let active = Some(tab.id()) == active_tab_id;
                    li class=(if active {
                        "viewer-tab active flex min-w-[112px] max-w-60 items-center rounded-t-panel border border-b-0 border-line-2 bg-bg text-ink shadow-[inset_0_2px_0_var(--acc)] [@media(max-width:760px)]:min-w-24"
                    } else {
                        "viewer-tab flex min-w-[112px] max-w-60 items-center rounded-t-panel border border-b-0 border-transparent bg-surface-2 text-ink-2 hover:border-line-2 hover:text-ink [@media(max-width:760px)]:min-w-24"
                    }) {
                        button type="button"
                            class="viewer-tab-activate flex min-w-0 flex-1 cursor-pointer items-center gap-[7px] border-0 bg-transparent py-2 pr-1 pl-2.5 text-left text-inherit [font:inherit] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
                            aria-current=[active.then_some("page")]
                            autofocus[active && feedback == SwapFeedback::LiveViewDeleted]
                            title=(tab.label())
                            hx-get=(ViewerRoute::Activate { tab: tab.id() })
                            hx-target="#viewer-view"
                            hx-swap="outerHTML" {
                            span class="viewer-tab-kind inline-flex size-[17px] flex-none items-center justify-center rounded-sm border border-line-2 text-[9px] font-bold text-ink-3" aria-hidden="true" { (tab_kind_label(tab.kind())) }
                            span class="viewer-tab-label min-w-0 truncate text-[12.5px]" { (tab.label()) }
                            (tab_state_marker(tab.state()))
                        }
                        button type="button"
                            class="viewer-tab-close mr-[3px] cursor-pointer rounded-sm border-0 bg-transparent px-1.5 py-[3px] text-[17px] leading-none text-ink-3 [font:inherit] hover:bg-del-bg hover:text-del focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
                            aria-label={ "Close " (tab.label()) }
                            title="Close tab"
                            hx-get=(ViewerRoute::Close { tab: tab.id() })
                            hx-target="#viewer-tabs"
                            hx-swap="outerHTML" { "×" }
                    }
                }
            }
            button type="button"
                class="viewer-history-button mb-[7px] flex flex-none cursor-pointer items-center gap-[7px] rounded-sm border border-transparent bg-transparent px-[9px] py-1.5 text-xs text-ink-2 [font:inherit] hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
                hx-get=(ViewerRoute::History)
                hx-target="#viewer-history"
                hx-swap="outerHTML"
                popovertarget="viewer-history-popover" {
                "History"
                @if !tabs.is_empty() {
                    span class="viewer-count min-w-[18px] rounded-[9px] bg-acc-soft px-[5px] text-center text-[10px] text-acc" { (tabs.len()) }
                }
            }
            @if let SwapFeedback::SnapshotRecipesSkipped(labels) = feedback {
                div class="gtl-toast viewer-toast-skip show pointer-events-none fixed bottom-6 left-1/2 z-50 flex max-w-[min(760px,calc(100vw-32px))] -translate-x-1/2 items-center gap-2 rounded-panel border border-acc-line bg-surface px-3.5 py-2 text-[12.5px] text-ink opacity-100 shadow-[0_10px_30px_rgba(0,0,0,.45)] [overflow-wrap:anywhere] before:font-bold before:text-acc before:content-['!']"
                    data-viewer-toast
                    role="status"
                    aria-live="polite"
                    aria-atomic="true" {
                    "Skipped " (labels.len()) " "
                    (if labels.len() == 1 { "diff" } else { "diffs" })
                    " with no commits or changed files: "
                    @for (index, label) in labels.iter().enumerate() {
                        @if index > 0 { ", " }
                        (label)
                    }
                    "."
                }
            }
            @if feedback == SwapFeedback::LiveViewDeleted {
                div class="viewer-sr-only sr-only" role="status" aria-live="polite" aria-atomic="true" {
                    @match tabs.iter().find(|tab| Some(tab.id()) == active_tab_id) {
                        Some(tab) => { "Live view deleted. Focus moved to " (tab.label()) "." }
                        None => { "Live view deleted. No diffs remain open. Open History or run gtl diff live to add one." }
                    }
                }
            }
        }
    }
}

fn tab_kind_label(kind: ViewerTabKind) -> &'static str {
    match kind {
        ViewerTabKind::Snapshot => "S",
        ViewerTabKind::Live => "L",
    }
}

fn tab_state_marker(state: &ViewerTabState) -> Markup {
    match state {
        ViewerTabState::Ready => html! {},
        ViewerTabState::Broken { .. } => html! {
            span class="viewer-tab-state broken inline-flex size-[15px] flex-none items-center justify-center rounded-full bg-acc-soft text-[10px] font-bold text-acc" aria-label="Unavailable" title="Unavailable" { "!" }
        },
        ViewerTabState::Error { .. } => html! {
            span class="viewer-tab-state error inline-flex size-[15px] flex-none items-center justify-center rounded-full bg-del-bg text-[10px] font-bold text-del" aria-label="Render failed" title="Render failed" { "×" }
        },
    }
}
