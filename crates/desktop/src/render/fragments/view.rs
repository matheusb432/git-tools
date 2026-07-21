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
    html! {
        section id="viewer-view" hx-swap-oob=[swap.out_of_band()] class="viewer-view" data-tab-id=[document.active_tab_id().map(|id| id.to_string())] {
            @match document.active_tab() {
                None => (empty_view(feedback == SwapFeedback::LiveViewDeleted)),
                Some(tab) => @match tab.state() {
                    ViewerTabState::Ready => {
                        @let view = document.active_view().expect("ViewerDocument guarantees a view for the ready active tab");
                        (controls::view_controls(view, document.settings()))
                        (preview::view_fragment(view.view(), view.options()))
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
        div.viewer-status.viewer-status-broken role="status" {
            span.viewer-status-mark aria-hidden="true" { "!" }
            div {
                p.viewer-status-eyebrow { "Unavailable · " code { (code) } }
                h1 { "This diff cannot be opened" }
                p { (reason) }
                button type="button"
                    class="viewer-recovery-button"
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
        div.viewer-status.viewer-status-error role="alert" {
            span.viewer-status-mark aria-hidden="true" { "×" }
            div {
                p.viewer-status-eyebrow { "Render failed" }
                h1 { "The diff could not be rendered" }
                p { (reason) }
                button type="button"
                    class="viewer-recovery-button"
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
        div.viewer-status.viewer-status-empty {
            span.viewer-status-mark aria-hidden="true" { "±" }
            div {
                p.viewer-status-eyebrow { "Viewer ready" }
                h1 { "No diff open" }
                p { "Run " code { "gtl diff" } " in a repository, or choose a previous render from History." }
                button type="button" class="viewer-recovery-button" autofocus[focus_history] popovertarget="viewer-history-popover" { "Open history" }
            }
        }
    }
}
