//! The render-history list shown inside the history popover.

use application::viewer::ViewerHistoryEntry;
use maud::{Markup, html};

use crate::render::ViewerRoute;

pub(in crate::render) fn history(entries: &[ViewerHistoryEntry]) -> Markup {
    html! {
        section id="viewer-history" class="viewer-history" aria-label="Recent diff previews" {
            @if entries.is_empty() {
                div.viewer-history-empty {
                    strong { "No history yet" }
                    p { "Rendered diffs will appear here after you open them." }
                }
            } @else {
                div.viewer-history-columns aria-hidden="true" {
                    span { "Diff" }
                    span { "Repository" }
                    span { "Kind" }
                    span { "Range" }
                    span { "Rendered" }
                }
                div.viewer-history-list {
                    @for entry in entries {
                        button type="button"
                            class="viewer-history-row"
                            hx-get=(ViewerRoute::OpenHistory { render: entry.id() })
                            hx-target="#viewer-tabs"
                            hx-swap="outerHTML"
                            popovertarget="viewer-history-popover"
                            popovertargetaction="hide" {
                            span.viewer-history-title { (entry.title()) }
                            span.viewer-history-repo { (entry.repo_name()) }
                            span.viewer-history-kind { (entry.kind()) }
                            span.viewer-history-range { (entry.range_label()) }
                            time datetime=(entry.rendered_at()) { (entry.rendered_at()) }
                        }
                    }
                }
            }
        }
    }
}
