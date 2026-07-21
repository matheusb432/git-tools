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
        nav id="viewer-tabs" hx-swap-oob=[swap.out_of_band()] class="viewer-tabs" aria-label="Open diffs" {
            ul.viewer-tab-list {
                @for tab in tabs {
                    @let active = Some(tab.id()) == active_tab_id;
                    li class=(if active { "viewer-tab active" } else { "viewer-tab" }) {
                        button type="button"
                            class="viewer-tab-activate"
                            aria-current=[active.then_some("page")]
                            autofocus[active && feedback == SwapFeedback::LiveViewDeleted]
                            title=(tab.label())
                            hx-get=(ViewerRoute::Activate { tab: tab.id() })
                            hx-target="#viewer-view"
                            hx-swap="outerHTML" {
                            span.viewer-tab-kind aria-hidden="true" { (tab_kind_label(tab.kind())) }
                            span.viewer-tab-label { (tab.label()) }
                            (tab_state_marker(tab.state()))
                        }
                        button type="button"
                            class="viewer-tab-close"
                            aria-label={ "Close " (tab.label()) }
                            title="Close tab"
                            hx-get=(ViewerRoute::Close { tab: tab.id() })
                            hx-target="#viewer-tabs"
                            hx-swap="outerHTML" { "×" }
                    }
                }
            }
            button type="button"
                class="viewer-history-button"
                hx-get=(ViewerRoute::History)
                hx-target="#viewer-history"
                hx-swap="outerHTML"
                popovertarget="viewer-history-popover" {
                "History"
                @if !tabs.is_empty() {
                    span.viewer-count { (tabs.len()) }
                }
            }
            @if let SwapFeedback::SnapshotRecipesSkipped(labels) = feedback {
                div class="gtl-toast viewer-toast-skip show"
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
                div class="viewer-sr-only" role="status" aria-live="polite" aria-atomic="true" {
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
            span.viewer-tab-state.broken aria-label="Unavailable" title="Unavailable" { "!" }
        },
        ViewerTabState::Error { .. } => html! {
            span.viewer-tab-state.error aria-label="Render failed" title="Render failed" { "×" }
        },
    }
}
