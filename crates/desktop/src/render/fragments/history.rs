//! The render-history list shown inside the history popover.

use application::viewer::ViewerHistoryEntry;
use maud::{Markup, html};

use crate::render::ViewerRoute;

pub(in crate::render) fn history(entries: &[ViewerHistoryEntry]) -> Markup {
    html! {
        section id="viewer-history" class="viewer-history h-[calc(100%-58px)] overflow-auto px-4 py-3.5 [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft" aria-label="Recent diff previews" {
            @if entries.is_empty() {
                div class="viewer-history-empty grid h-full place-content-center text-center text-ink-2" {
                    strong class="text-ink" { "No history yet" }
                    p class="mt-1 mb-0" { "Rendered diffs will appear here after you open them." }
                }
            } @else {
                div class="viewer-history-columns grid grid-cols-[minmax(180px,1.5fr)_minmax(130px,1fr)_90px_minmax(130px,1fr)_170px] items-center gap-3 border-b border-line px-2.5 pt-1.5 pb-2 text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase [@media(max-width:760px)]:hidden" aria-hidden="true" {
                    span { "Diff" }
                    span { "Repository" }
                    span { "Kind" }
                    span { "Range" }
                    span { "Rendered" }
                }
                div class="viewer-history-list pt-1" {
                    @for entry in entries {
                        button type="button"
                            class="viewer-history-row grid w-full cursor-pointer grid-cols-[minmax(180px,1.5fr)_minmax(130px,1fr)_90px_minmax(130px,1fr)_170px] items-center gap-3 rounded-sm border border-transparent bg-transparent px-2.5 py-[9px] text-left text-ink-2 [font:inherit] hover:border-line hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc [@media(max-width:760px)]:grid-cols-[minmax(0,1fr)_auto]"
                            hx-get=(ViewerRoute::OpenHistory { render: entry.id() })
                            hx-target="#viewer-tabs"
                            hx-swap="outerHTML"
                            popovertarget="viewer-history-popover"
                            popovertargetaction="hide" {
                            span class="viewer-history-title min-w-0 truncate font-semibold text-ink" { (entry.title()) }
                            span class="viewer-history-repo min-w-0 truncate [@media(max-width:760px)]:hidden" { (entry.repo_name()) }
                            span class="viewer-history-kind w-max min-w-0 truncate rounded-sm border border-line-2 px-1.5 py-px text-[10px] text-acc" { (entry.kind()) }
                            span class="viewer-history-range min-w-0 truncate text-[11px] tabular-nums [@media(max-width:760px)]:hidden" { (entry.range_label()) }
                            time class="min-w-0 truncate text-[11px] tabular-nums [@media(max-width:760px)]:hidden" datetime=(entry.rendered_at()) { (entry.rendered_at()) }
                        }
                    }
                }
            }
        }
    }
}
