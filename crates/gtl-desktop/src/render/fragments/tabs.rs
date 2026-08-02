//! The tab strip: one tab per open diff, the History and Theme buttons, and the
//! accessible feedback (skip toast, deletion announcement) a compound
//! response attaches to it.

use gtl_application::viewer::{Theme, ViewerTab, ViewerTabId, ViewerTabKind, ViewerTabState};
use maud::{Markup, html};

use super::{SwapFeedback, SwapMode, theme};
use crate::{render::ViewerRoute, session::RENDER_PENDING_REASON};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::render) struct MobileNavigationCounts {
    pub(in crate::render) files: usize,
    pub(in crate::render) commits: usize,
}

pub(in crate::render) fn tabs(
    tabs: &[ViewerTab],
    active_tab_id: Option<ViewerTabId>,
    active_theme: Theme,
    mobile_counts: Option<MobileNavigationCounts>,
    swap: SwapMode,
    feedback: SwapFeedback<'_>,
) -> Markup {
    html! {
        nav id="viewer-tabs" hx-swap-oob=[swap.out_of_band()] class="viewer-tabs z-[70] flex min-w-0 items-end gap-2.5 border-b border-line bg-surface px-3 pt-2 [&.htmx-swapping]:border-acc-line [&.htmx-settling]:border-acc-line mobile:grid mobile:h-[58px] mobile:grid-cols-[58px_minmax(0,1fr)_58px_58px] mobile:items-stretch mobile:gap-0 mobile:p-0" aria-label="Open diffs" {
            (gtl_preview::files_navigation(
                mobile_counts.map(|counts| counts.files),
                mobile_counts.is_some(),
            ))
            ul class="viewer-tab-list gtl-scroll-rail m-0 flex min-w-0 flex-1 list-none items-end gap-1 overflow-x-auto p-0 mobile:h-full mobile:items-stretch" {
                @for tab in tabs {
                    @let active = Some(tab.id()) == active_tab_id;
                    li class=(if active {
                        "viewer-tab active flex min-w-[112px] max-w-60 items-center rounded-t-panel border border-b-0 border-line-2 bg-bg text-ink shadow-[inset_0_2px_0_var(--acc)] mobile:min-w-24 mobile:rounded-none mobile:border-0 mobile:bg-transparent mobile:shadow-[inset_0_-2px_0_var(--acc)]"
                    } else {
                        "viewer-tab flex min-w-[112px] max-w-60 items-center rounded-t-panel border border-b-0 border-transparent bg-surface-2 text-ink-2 hover:border-line-2 hover:text-ink mobile:min-w-24 mobile:rounded-none mobile:border-0 mobile:bg-transparent"
                    })
                        aria-busy=[matches!(tab.state(), ViewerTabState::Error { reason } if reason == RENDER_PENDING_REASON).then_some("true")] {
                        button type="button"
                            class={
                                "viewer-tab-activate flex min-w-0 flex-1 items-center gap-[7px] border-0 bg-transparent py-2 pr-1 pl-2.5 text-left text-inherit [font:inherit] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc "
                                (if active { "" } else { "cursor-pointer" })
                            }
                            aria-current=[active.then_some("page")]
                            aria-disabled=[active.then_some("true")]
                            autofocus[active && feedback == SwapFeedback::LiveViewDeleted]
                            title=(tab.label())
                            hx-get=[(!active).then(|| ViewerRoute::Activate { tab: tab.id() })]
                            hx-target=[(!active).then_some("#viewer-view")]
                            hx-sync=[(!active).then_some("#viewer-view:replace")]
                            hx-swap=[(!active).then_some("outerHTML")] {
                            span class="viewer-tab-kind inline-flex size-[17px] flex-none items-center justify-center rounded-sm border border-line-2 text-[9px] font-bold text-ink-3" aria-hidden="true" { (tab_kind_label(tab.kind())) }
                            span class="viewer-tab-label min-w-0 truncate text-[12.5px]" { (tab.label()) }
                            (tab_state_marker(tab.state()))
                        }
                        button type="button"
                            class="viewer-tab-close mr-[3px] cursor-pointer rounded-sm border-0 bg-transparent px-1.5 py-[3px] text-[17px] leading-none text-ink-3 [font:inherit] hover:bg-del-bg hover:text-del focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
                            aria-label={ "Close " (tab.label()) }
                            title="Close tab"
                            hx-get=(ViewerRoute::Close { tab: tab.id() })
                            hx-target=(if active { "#viewer-view" } else { "#viewer-tabs" })
                            hx-sync=[active.then_some("#viewer-view:replace")]
                            hx-swap="outerHTML" { "×" }
                    }
                }
            }
            (gtl_preview::commits_navigation(
                mobile_counts.map(|counts| counts.commits),
                mobile_counts.is_some(),
            ))
            (gtl_preview::view_navigation(
                "viewer-controls-popover",
                mobile_counts.is_some(),
            ))
            button type="button"
                class="viewer-history-button mb-[7px] flex flex-none cursor-pointer items-center gap-[7px] rounded-sm border border-transparent bg-transparent px-[9px] py-1.5 text-xs text-ink-2 [font:inherit] hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc mobile:hidden"
                hx-get=(ViewerRoute::History)
                hx-target="#viewer-history"
                hx-swap="outerHTML"
                popovertarget="viewer-history-popover" {
                "History"
                @if !tabs.is_empty() {
                    span class="viewer-count min-w-[18px] rounded-[9px] bg-acc-soft px-[5px] text-center text-[10px] text-acc" { (tabs.len()) }
                }
            }
            (theme::trigger(active_theme))
            @if let SwapFeedback::SnapshotRecipesSkipped(labels) = feedback {
                div class="gtl-toast viewer-toast-skip pointer-events-none fixed bottom-6 left-1/2 z-50 flex max-w-[min(760px,calc(100vw-32px))] -translate-x-1/2 items-center gap-2 rounded-panel border border-acc-line bg-surface px-3.5 py-2 text-[12.5px] text-ink opacity-100 shadow-[0_6px_18px_rgba(0,0,0,.22)] transition-[opacity,scale] duration-200 ease-out starting:scale-95 starting:opacity-0 motion-reduce:transition-none [overflow-wrap:anywhere] before:font-bold before:text-acc before:content-['!'] [&[data-leaving]]:scale-95 [&[data-leaving]]:opacity-0"
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
