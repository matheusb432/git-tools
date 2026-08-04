//! The keyset-paginated render history shown inside the history popover.

use std::num::NonZeroUsize;

use gtl_application::{
    history::list_recent_render_page::{RECENT_RENDER_PAGE_SIZE, RecentRenderPageCursor},
    viewer::{ViewerHistoryEntry, ViewerHistoryPage},
};
use maud::{Markup, PreEscaped, html};

use crate::render::ViewerRoute;

const COPY_ICON: &str = r#"<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"></rect><path d="M10.5 5.5V4A1.5 1.5 0 0 0 9 2.5H4A1.5 1.5 0 0 0 2.5 4v5A1.5 1.5 0 0 0 4 10.5h1.5"></path></svg>"#;

/// Builds the JSON object the copy button writes to the clipboard: the record
/// with its recipe serialized as a nested object.
fn history_copy_json(entry: &ViewerHistoryEntry) -> String {
    let recipe = serde_json::to_value(entry.recipe()).unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "id": i64::from(entry.id()),
        "title": entry.title(),
        "repo_name": entry.repo_name(),
        "kind": entry.kind(),
        "range_label": entry.range_label(),
        "rendered_at": entry.rendered_at(),
        "recipe": recipe,
    });
    serde_json::to_string_pretty(&payload).unwrap_or_default()
}

pub(in crate::render) fn history(page: &ViewerHistoryPage) -> Markup {
    let entries = page.entries();
    html! {
        section id="viewer-history" class="viewer-history grid h-[calc(100%-58px)] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden px-4 [&.htmx-swapping]:bg-acc-soft [&.htmx-settling]:bg-acc-soft" aria-label="Recent diff previews" {
            @if entries.is_empty() {
                div class="viewer-history-empty col-span-full row-span-full grid h-full place-content-center text-center text-ink-2" {
                    strong class="text-ink" { "No history yet" }
                    p class="mt-1 mb-0" { "Rendered diffs will appear here after you open them." }
                }
            } @else {
                div class="viewer-history-columns grid grid-cols-[52px_minmax(160px,1.5fr)_minmax(120px,1fr)_88px_minmax(120px,1fr)_168px_34px] items-center gap-3 border-b border-line px-2.5 pt-5 pb-2 text-[10px] font-bold tracking-[.06em] text-ink-3 uppercase mobile:hidden" aria-hidden="true" {
                    span { "ID" }
                    span { "Diff" }
                    span { "Repository" }
                    span { "Kind" }
                    span { "Range" }
                    span { "Rendered" }
                    span {}
                }
                div class="viewer-history-list gtl-scroll min-h-0 overflow-auto pt-1" {
                    @for entry in entries {
                        div class="viewer-history-row grid w-full cursor-pointer select-text grid-cols-[52px_minmax(160px,1.5fr)_minmax(120px,1fr)_88px_minmax(120px,1fr)_168px_34px] items-center gap-3 rounded-sm border border-transparent px-2.5 py-2 text-ink-2 hover:border-line hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc mobile:grid-cols-[minmax(0,1fr)_auto_34px]"
                            role="button"
                            tabindex="0"
                            hx-get=(ViewerRoute::OpenHistory { render: entry.id() })
                            hx-target="#viewer-tabs"
                            hx-sync="#viewer-view:replace"
                            hx-swap="outerHTML" {
                            span class="viewer-history-id min-w-0 truncate text-[11px] tabular-nums text-ink-3 mobile:hidden" { "#" (i64::from(entry.id())) }
                            span class="viewer-history-title min-w-0 truncate font-semibold text-ink" { (entry.title()) }
                            span class="viewer-history-repo min-w-0 truncate mobile:hidden" { (entry.repo_name()) }
                            span class="viewer-history-kind w-max min-w-0 truncate rounded-sm border border-line-2 px-1.5 py-px text-[10px] text-acc" { (entry.kind()) }
                            span class="viewer-history-range min-w-0 truncate text-[11px] tabular-nums mobile:hidden" { (entry.range_label()) }
                            time class="min-w-0 truncate text-[11px] tabular-nums mobile:hidden" datetime=(entry.rendered_at()) { (entry.rendered_at()) }
                            button type="button"
                                class="viewer-history-copy grid size-[26px] cursor-pointer place-content-center justify-self-center rounded-sm border-0 bg-transparent text-ink-3 select-none hover:bg-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc [&[data-copied]]:text-add"
                                data-history-copy=(history_copy_json(entry))
                                aria-label="Copy render JSON"
                                title="Copy render JSON" {
                                (PreEscaped(COPY_ICON))
                            }
                        }
                    }
                }
                (history_footer(page))
            }
        }
    }
}

fn history_footer(page: &ViewerHistoryPage) -> Markup {
    let first_render = page.entries().first().map(ViewerHistoryEntry::id);
    let last_render = page.entries().last().map(ViewerHistoryEntry::id);
    let previous_page = page
        .page_number()
        .checked_sub(1)
        .and_then(NonZeroUsize::new);
    let next_page = page
        .page_number()
        .checked_add(1)
        .and_then(NonZeroUsize::new);
    let first_route = page.has_newer().then_some(ViewerRoute::History {
        cursor: RecentRenderPageCursor::Newest,
    });
    let previous_route = page
        .has_newer()
        .then_some(())
        .and_then(|()| first_render.zip(previous_page))
        .map(|(render, page)| ViewerRoute::History {
            cursor: RecentRenderPageCursor::NewerThan { render, page },
        });
    let next_route = page
        .has_older()
        .then_some(())
        .and_then(|()| last_render.zip(next_page))
        .map(|(render, page)| ViewerRoute::History {
            cursor: RecentRenderPageCursor::OlderThan { render, page },
        });
    let last_route = page.has_older().then_some(ViewerRoute::History {
        cursor: RecentRenderPageCursor::Oldest,
    });
    let progress_percent = page
        .page_number()
        .checked_mul(100)
        .and_then(|value| value.checked_div(page.page_count()))
        .unwrap_or(0);
    html! {
        footer class="viewer-history-footer relative -mx-4 grid min-h-14 grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center border-t border-line bg-surface px-4 mobile:grid-cols-[1fr_auto] mobile:px-2" {
            div class="absolute inset-x-4 top-0 h-px bg-line mobile:inset-x-2" role="progressbar" aria-label="History page position" aria-valuemin="1" aria-valuemax=(page.page_count()) aria-valuenow=(page.page_number()) {
                span class="block h-full bg-acc" style=(format!("width:{progress_percent}%")) {}
            }
            p class="m-0 text-[11px] tabular-nums text-ink-3 mobile:hidden" {
                (page.total_count()) " renders " (PreEscaped("&middot;")) " " (RECENT_RENDER_PAGE_SIZE) " per page"
            }
            nav class="col-start-2 flex items-center justify-center gap-1" aria-label="Render history pages" {
                (history_page_button("First page", "<<", first_route))
                (history_page_button("Previous page", "<", previous_route))
                output class="min-w-16 px-2 text-center text-xs tabular-nums text-ink" aria-label=(format!("Page {} of {}", page.page_number(), page.page_count())) {
                    (format!("{:02}", page.page_number())) " / " (format!("{:02}", page.page_count()))
                }
                (history_page_button("Next page", ">", next_route))
                (history_page_button("Last page", ">>", last_route))
            }
        }
    }
}

fn history_page_button(label: &str, icon: &str, route: Option<ViewerRoute>) -> Markup {
    let disabled = route.is_none();
    html! {
        button type="button"
            class="viewer-history-page-button grid min-h-11 min-w-11 cursor-pointer place-content-center rounded-sm border border-transparent bg-transparent px-2 text-xs font-semibold text-ink-2 [font:inherit] hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc disabled:cursor-default disabled:text-ink-3 disabled:opacity-50 disabled:hover:border-transparent disabled:hover:bg-transparent [&.htmx-request]:cursor-progress [&.htmx-request]:border-acc-line [&.htmx-request]:bg-acc-soft [&.htmx-request]:text-acc"
            aria-label=(label)
            title=(label)
            disabled[disabled]
            hx-get=[route.map(|route| route.to_string())]
            hx-target="#viewer-history"
            hx-swap="outerHTML"
            hx-sync="#viewer-history:replace" {
            span aria-hidden="true" { (icon) }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::{RenderHistoryId, ViewerHistoryEntry};
    use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};

    use super::history_copy_json;

    fn entry() -> ViewerHistoryEntry {
        ViewerHistoryEntry::new(
            RenderHistoryId::try_new(9).expect("positive id"),
            "Recent changes".into(),
            "git-tools".into(),
            "main..HEAD".into(),
            "2026-07-11T00:00:00Z".into(),
            Recipe {
                source: RecipeSource::LocalRepo("/repos/gt".into()),
                op: RecipeOp::Diff {
                    target: RecipeTarget::Unpushed { pinned: None },
                },
                name: None,
            },
        )
    }

    #[test]
    fn copy_json_wraps_the_record_with_a_nested_recipe_object() {
        let payload: serde_json::Value =
            serde_json::from_str(&history_copy_json(&entry())).expect("copy payload is valid json");

        assert_eq!(payload["id"], 9);
        assert_eq!(payload["title"], "Recent changes");
        assert_eq!(payload["repo_name"], "git-tools");
        assert_eq!(payload["kind"], "diff");
        assert_eq!(payload["range_label"], "main..HEAD");
        assert_eq!(
            payload["recipe"],
            serde_json::json!({
                "source": { "kind": "local_repo", "value": "/repos/gt" },
                "op": { "op": "diff", "target": { "target": "unpushed" } },
            })
        );
    }
}
