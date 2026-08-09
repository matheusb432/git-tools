use dioxus::prelude::*;
use gtl_contracts::viewer::{ViewerHistoryCursor, ViewerHistoryEntry, ViewerHistoryPage};
use lucide_dioxus::{
    Check, ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Copy, ExternalLink, History,
};

use crate::{
    app::{application_layout::ViewerContext, application_router::Route},
    entities::diffs::{ViewerApi, history_navigation, recipe_kind_label},
    shared::{
        bridge::ClientApiError,
        browser,
        ui::{Button, ButtonSize, ButtonState, ButtonVariant, Skeleton},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum HistoryLoad {
    Loading,
    Ready(ViewerHistoryPage),
    Error(ClientApiError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HistoryOpenRequest {
    render_id: i64,
    generation: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HistoryOpenState {
    generation: u64,
    request: Option<HistoryOpenRequest>,
}

impl HistoryOpenState {
    fn begin(&mut self, render_id: i64) -> Option<HistoryOpenRequest> {
        if self.request.is_some() {
            return None;
        }

        self.generation = self.generation.wrapping_add(1);
        let request = HistoryOpenRequest {
            render_id,
            generation: self.generation,
        };
        self.request = Some(request);
        Some(request)
    }

    const fn accepts(self, request: HistoryOpenRequest) -> bool {
        matches!(self.request, Some(current) if current.render_id == request.render_id && current.generation == request.generation)
    }

    fn finish(&mut self, request: HistoryOpenRequest) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.request = None;
        true
    }

    const fn is_pending(self) -> bool {
        self.request.is_some()
    }

    const fn is_opening(self, render_id: i64) -> bool {
        matches!(self.request, Some(request) if request.render_id == render_id)
    }
}

#[component]
pub(crate) fn DiffHistoryView() -> Element {
    let viewer = use_context::<ViewerContext>();
    let navigator = use_navigator();
    let mut cursor = use_signal(|| ViewerHistoryCursor::Newest);
    let mut reload = use_signal(|| 0_u64);
    let mut query_generation = use_signal(|| 0_u64);
    let mut history = use_signal(|| HistoryLoad::Loading);
    let mut action_error = use_signal(|| None::<ClientApiError>);
    let mut open_state = use_signal(HistoryOpenState::default);
    let mut copied_id = use_signal(|| None::<i64>);

    use_effect(move || {
        browser::focus_element("history-heading".into());
    });
    use_effect(move || {
        let requested_cursor = cursor();
        let _reload = reload();
        let generation = {
            let mut generation = query_generation.write();
            *generation += 1;
            *generation
        };
        history.set(HistoryLoad::Loading);
        spawn(async move {
            let result = ViewerApi::list_history(requested_cursor).await;
            if query_generation() != generation {
                return;
            }
            history.set(match result {
                Ok(page) => HistoryLoad::Ready(page),
                Err(error) => HistoryLoad::Error(error),
            });
        });
    });

    let load = history();

    rsx! {
        document::Title { "History - git-tools" }
        main { class: "grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden",
            header { class: "flex flex-col gap-3 border-b border-line bg-surface px-4 py-4 sm:flex-row sm:items-end sm:justify-between",
                div { class: "min-w-0",
                    div { class: "flex items-center gap-2 text-acc",
                        span { aria_hidden: "true", History { size: 16 } }
                        p { class: "font-mono text-[10px] font-semibold uppercase tracking-[0.14em]", "Render archive" }
                    }
                    h1 { id: "history-heading", class: "mt-1 text-lg font-semibold tracking-tight text-ink focus:outline-none", tabindex: "-1", "Diff history" }
                    p { class: "mt-1 text-xs text-ink-2", "Reopen a durable render or copy its complete recipe." }
                }
                if let HistoryLoad::Ready(page) = &load {
                    p { class: "font-mono text-[11px] tabular-nums text-ink-3", "{page.total_count} renders" }
                }
            }

            section { class: "grid min-h-0 grid-rows-[minmax(0,1fr)_auto] bg-bg", aria_label: "Recent diff renders",
                div { class: "min-h-0 overflow-auto p-3 [scrollbar-color:var(--color-line-2)_transparent] [scrollbar-width:thin] sm:p-4",
                    if let Some(error) = action_error() {
                        div { class: "mb-3 rounded-sm border border-del-line bg-del-bg px-3 py-2 text-xs text-del", role: "alert",
                            "{error.message()}"
                        }
                    }
                    match &load {
                        HistoryLoad::Loading => rsx! { HistoryLoading {} },
                        HistoryLoad::Error(error) => {
                            let message = error.message();
                            rsx! {
                                div { class: "grid min-h-64 place-content-center text-center", role: "alert",
                                    p { class: "text-sm font-semibold text-ink", "History is unavailable" }
                                    p { class: "mt-1 max-w-md text-xs leading-5 text-ink-2", "{message}" }
                                    Button { class: "mx-auto mt-4", variant: ButtonVariant::Outline, onclick: move |_| *reload.write() += 1, "Try again" }
                                }
                            }
                        },
                        HistoryLoad::Ready(page) if page.entries.is_empty() => rsx! {
                            div { class: "grid min-h-64 place-content-center text-center",
                                p { class: "text-sm font-semibold text-ink", "No history yet" }
                                p { class: "mt-1 text-xs text-ink-2", "Rendered diffs appear here after they are opened." }
                            }
                        },
                        HistoryLoad::Ready(page) => rsx! {
                            div { class: "grid gap-2",
                                for entry in &page.entries {
                                    HistoryRow {
                                        key: "{entry.id}",
                                        entry: entry.clone(),
                                        opening: open_state().is_opening(entry.id),
                                        open_disabled: open_state().is_pending(),
                                        copied: copied_id() == Some(entry.id),
                                        onopen: move |render_id: i64| {
                                            let Some(open_request) = open_state.write().begin(render_id) else {
                                                return;
                                            };
                                            action_error.set(None);
                                            spawn(async move {
                                                let result = ViewerApi::open_history(render_id).await;
                                                if !open_state().accepts(open_request) {
                                                    return;
                                                }
                                                let _ = open_state.write().finish(open_request);
                                                match result {
                                                    Ok(shell) => {
                                                        viewer.replace_shell(shell);
                                                        navigator.push(Route::Workspace {});
                                                    }
                                                    Err(error) => action_error.set(Some(error)),
                                                }
                                            });
                                        },
                                        oncopy: move |render_id: i64| {
                                            action_error.set(None);
                                            spawn(async move {
                                                match ViewerApi::get_history_copy(render_id).await {
                                                    Ok(payload) => match browser::copy_json(&payload).await {
                                                        Ok(()) => copied_id.set(Some(render_id)),
                                                        Err(error) => action_error.set(Some(error)),
                                                    },
                                                    Err(error) => action_error.set(Some(error)),
                                                }
                                            });
                                        },
                                    }
                                }
                            }
                        },
                    }
                }
                if let HistoryLoad::Ready(page) = &load && !page.entries.is_empty() {
                    HistoryFooter { page: page.clone(), onnavigate: move |next| cursor.set(next) }
                }
            }
        }
    }
}

#[component]
fn HistoryLoading() -> Element {
    rsx! {
        div { class: "grid gap-2", role: "status", aria_label: "Loading history",
            for _ in 0..6 {
                div { class: "grid gap-2 rounded-sm border border-line bg-surface p-3 sm:grid-cols-[minmax(0,1.5fr)_minmax(8rem,1fr)_auto]",
                    Skeleton { class: "h-4 w-3/5" }
                    Skeleton { class: "h-4 w-4/5" }
                    Skeleton { class: "h-8 w-24" }
                }
            }
            span { class: "sr-only", "Loading history" }
        }
    }
}

#[component]
fn HistoryRow(
    entry: ViewerHistoryEntry,
    opening: bool,
    open_disabled: bool,
    copied: bool,
    onopen: EventHandler<i64>,
    oncopy: EventHandler<i64>,
) -> Element {
    rsx! {
        article { class: "grid min-w-0 gap-3 rounded-sm border border-line bg-surface px-3 py-3 hover:border-line-2 sm:grid-cols-[minmax(0,1.4fr)_minmax(8rem,0.8fr)_auto] sm:items-center",
            div { class: "min-w-0",
                div { class: "flex min-w-0 items-center gap-2",
                    h2 { class: "truncate text-sm font-semibold text-ink", "{entry.title}" }
                    span { class: "shrink-0 rounded-sm border border-acc-line bg-acc-soft px-1.5 py-0.5 font-mono text-[9px] font-semibold uppercase tracking-[0.08em] text-acc", "{recipe_kind_label(entry.kind)}" }
                }
                p { class: "mt-1 truncate font-mono text-[11px] text-ink-2", "{entry.range_label}" }
            }
            div { class: "min-w-0 text-[11px] text-ink-2",
                p { class: "truncate", "{entry.repository_name}" }
                time { class: "mt-1 block truncate font-mono tabular-nums text-ink-3", datetime: entry.rendered_at.clone(), "{entry.rendered_at}" }
            }
            div { class: "flex items-center justify-end gap-1",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    state: if opening { ButtonState::Loading } else if open_disabled { ButtonState::Disabled } else { ButtonState::Enabled },
                    aria_label: "Open {entry.title}",
                    onclick: move |_| onopen.call(entry.id),
                    span { aria_hidden: "true", ExternalLink { size: 14 } }
                    "Open"
                }
                Button {
                    size: ButtonSize::IconSmall,
                    variant: ButtonVariant::Ghost,
                    aria_label: "Copy {entry.title} JSON",
                    title: if copied { "Copied" } else { "Copy render JSON" },
                    onclick: move |_| oncopy.call(entry.id),
                    span { aria_hidden: "true",
                        if copied { Check { size: 14 } } else { Copy { size: 14 } }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryFooter(
    page: ViewerHistoryPage,
    onnavigate: EventHandler<ViewerHistoryCursor>,
) -> Element {
    let navigation = history_navigation(&page);
    let progress = page
        .page_number
        .saturating_mul(100)
        .checked_div(page.page_count)
        .unwrap_or(0);

    rsx! {
        footer { class: "relative flex min-h-14 items-center justify-between gap-3 border-t border-line bg-surface px-3 sm:px-4",
            div { class: "absolute inset-x-3 top-0 h-px bg-line sm:inset-x-4", role: "progressbar", aria_label: "History page position", aria_valuemin: "1", aria_valuemax: page.page_count.to_string(), aria_valuenow: page.page_number.to_string(),
                span { class: "block h-full bg-acc", style: "width:{progress}%" }
            }
            p { class: "hidden font-mono text-[10px] tabular-nums text-ink-3 sm:block", "{page.total_count} renders" }
            nav { class: "ml-auto flex items-center gap-1", aria_label: "History pages",
                HistoryPageButton { label: "First page", cursor: navigation.first, onclick: onnavigate, icon: HistoryPageIcon::First }
                HistoryPageButton { label: "Previous page", cursor: navigation.previous, onclick: onnavigate, icon: HistoryPageIcon::Previous }
                output { class: "min-w-20 px-2 text-center font-mono text-xs tabular-nums text-ink", aria_label: "Page {page.page_number} of {page.page_count}", "{page.page_number:02} / {page.page_count:02}" }
                HistoryPageButton { label: "Next page", cursor: navigation.next, onclick: onnavigate, icon: HistoryPageIcon::Next }
                HistoryPageButton { label: "Last page", cursor: navigation.last, onclick: onnavigate, icon: HistoryPageIcon::Last }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryPageIcon {
    First,
    Previous,
    Next,
    Last,
}

#[component]
fn HistoryPageButton(
    label: String,
    cursor: Option<ViewerHistoryCursor>,
    onclick: EventHandler<ViewerHistoryCursor>,
    icon: HistoryPageIcon,
) -> Element {
    rsx! {
        Button {
            size: ButtonSize::IconMedium,
            variant: ButtonVariant::Ghost,
            state: if cursor.is_some() { ButtonState::Enabled } else { ButtonState::Disabled },
            aria_label: label.clone(),
            title: label,
            onclick: move |_| {
                if let Some(cursor) = cursor {
                    onclick.call(cursor);
                }
            },
            span { aria_hidden: "true",
                match icon {
                    HistoryPageIcon::First => rsx! { ChevronsLeft { size: 15 } },
                    HistoryPageIcon::Previous => rsx! { ChevronLeft { size: 15 } },
                    HistoryPageIcon::Next => rsx! { ChevronRight { size: 15 } },
                    HistoryPageIcon::Last => rsx! { ChevronsRight { size: 15 } },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HistoryOpenState;

    #[test]
    fn history_open_is_page_wide_and_rejects_stale_completion() {
        let mut state = HistoryOpenState::default();
        let first = super::HistoryOpenRequest {
            render_id: 9,
            generation: 1,
        };
        assert_eq!(state.begin(9), Some(first));

        assert!(state.is_pending());
        assert!(state.is_opening(9));
        assert_eq!(state.begin(10), None);

        assert!(state.finish(first));
        let second = super::HistoryOpenRequest {
            render_id: 10,
            generation: 2,
        };
        assert_eq!(state.begin(10), Some(second));
        assert!(!state.accepts(first));
        assert!(state.accepts(second));
    }
}
