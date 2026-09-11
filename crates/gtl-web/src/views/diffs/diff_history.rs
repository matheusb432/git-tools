use dioxus::prelude::*;
use gtl_models::viewer::RenderHistoryId;
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{
    GetViewerHistoryCopy, ListViewerHistory, OpenViewerHistory, ViewerHistoryCursor,
    ViewerHistoryEntry, ViewerHistoryPage,
};
use lucide_dioxus::{Check, ChevronLeft, ChevronRight, Copy, ExternalLink, History};

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::{history_navigation, recipe_kind_label, viewer_server},
    shared::{
        browser,
        ui::{Button, ButtonSize, ButtonState, ButtonVariant, PageNotice, ScrollArea, Skeleton},
        viewer_client::ViewerClientError,
    },
};

#[derive(Clone, Copy)]
struct HistoryActions {
    error: ReadSignal<Option<ViewerClientError>>,
    opening_id: ReadSignal<Option<RenderHistoryId>>,
    copied_id: ReadSignal<Option<RenderHistoryId>>,
    open: Callback<RenderHistoryId>,
    copy: Callback<RenderHistoryId>,
}

fn use_history_actions() -> HistoryActions {
    let viewer = use_context::<ViewerContext>();
    let mut error = use_signal(|| None::<ViewerClientError>);
    let mut opening_id = use_signal(|| None::<RenderHistoryId>);
    let mut copied_id = use_signal(|| None::<RenderHistoryId>);

    let mut open_action = use_action(move |render_id: RenderHistoryId| async move {
        let result = viewer_server::open_history(OpenViewerHistory { render_id }).await;
        opening_id.set(None);
        match result {
            Ok(shell) => {
                viewer.replace_shell(shell);
            }
            Err(next_error) => error.set(Some(next_error)),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let open = use_callback(move |render_id: RenderHistoryId| {
        if opening_id.peek().is_some() {
            return;
        }
        error.set(None);
        opening_id.set(Some(render_id));
        open_action.call(render_id);
    });

    let mut copy_action = use_action(move |render_id: RenderHistoryId| async move {
        match viewer_server::get_history_copy(GetViewerHistoryCopy { render_id }).await {
            Ok(payload) if browser::copy_text(&payload.json).await => {
                copied_id.set(Some(render_id));
            }
            Ok(_) => error.set(Some(ViewerClientError::Unavailable)),
            Err(next_error) => error.set(Some(next_error)),
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let copy = use_callback(move |render_id: RenderHistoryId| {
        error.set(None);
        copy_action.call(render_id);
    });

    HistoryActions {
        error: error.into(),
        opening_id: opening_id.into(),
        copied_id: copied_id.into(),
        open,
        copy,
    }
}

#[component]
pub(crate) fn DiffHistoryView() -> Element {
    let mut cursor = use_signal(|| ViewerHistoryCursor::Newest);
    let mut history = use_resource(move || {
        let cursor = cursor();
        async move { viewer_server::list_history(ListViewerHistory { cursor }).await }
    });
    let actions = use_history_actions();

    use_effect(move || {
        browser::focus_element("history-heading".into());
    });
    let pending = history.state().cloned() == UseResourceState::Pending;
    let load = history.read();
    let action_error = (actions.error)();
    let opening_id = (actions.opening_id)();
    let copied_id = (actions.copied_id)();

    rsx! {
        document::Title { "History - git-tools" }
        main { class: "history-shell h-full min-h-0",
            header { class: "history-header gap-3 px-4 py-4",
                div { class: "min-w-0",
                    div { class: "flex items-center gap-2 text-acc",
                        span { aria_hidden: "true",
                            History { size: 16 }
                        }
                        p { class: "font-mono font-semibold tracking-widest uppercase",
                            "Render archive"
                        }
                    }
                    h1 {
                        id: "history-heading",
                        class: "history-header-title mt-1 text-lg font-semibold tracking-tight",
                        tabindex: "-1",
                        "Diff history"
                    }
                    p { class: "mt-1 text-ink-2",
                        "Reopen a durable render or copy its complete recipe."
                    }
                }
                if !pending && let Some(Ok(page)) = &*load {
                    p { class: "font-mono tabular-nums text-ink-3", "{page.total_count} renders" }
                }
            }

            section {
                class: "history-content min-h-0",
                aria_label: "Recent diff renders",
                ScrollArea { class: "overflow-auto min-h-0 p-3 sm:p-4",
                    if let Some(error) = action_error {
                        div {
                            class: "history-error mb-3 px-3 py-2",
                            role: "alert",
                            "{error.message()}"
                        }
                    }
                    match (pending, &*load) {
                        (true, _) | (false, None) => rsx! {
                            HistoryLoading {}
                        },
                        (false, Some(Err(error))) => {
                            let message = error.message();
                            rsx! {
                                PageNotice {
                                    class: "min-h-64",
                                    role: "alert",
                                    title: "History is unavailable",
                                    message,
                                    Button {
                                        class: "mx-auto mt-4",
                                        variant: ButtonVariant::Outline,
                                        onclick: move |_| history.restart(),
                                        "Try again"
                                    }
                                }
                            }
                        }
                        (false, Some(Ok(page))) if page.entries.is_empty() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: "No history yet",
                                message: "Rendered diffs appear here after they are opened.",
                            }
                        },
                        (false, Some(Ok(page))) => rsx! {
                            div { class: "grid gap-2",
                                for entry in &page.entries {
                                    HistoryRow {
                                        key: "{entry.id}",
                                        entry: entry.clone(),
                                        opening: opening_id == Some(entry.id),
                                        open_disabled: opening_id.is_some(),
                                        copied: copied_id == Some(entry.id),
                                        onopen: move |render_id| actions.open.call(render_id),
                                        oncopy: move |render_id| actions.copy.call(render_id),
                                    }
                                }
                            }
                        },
                    }
                }
                if !pending && let Some(Ok(page)) = &*load && !page.entries.is_empty() {
                    HistoryFooter {
                        page: page.clone(),
                        onnavigate: move |next| cursor.set(next),
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryLoading() -> Element {
    rsx! {
        div {
            class: "grid gap-2",
            role: "status",
            aria_label: "Loading history",
            for _ in 0..6 {
                div { class: "history-loading-row gap-2 p-3",
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
    onopen: EventHandler<RenderHistoryId>,
    oncopy: EventHandler<RenderHistoryId>,
) -> Element {
    rsx! {
        article { class: "history-row min-w-0 gap-3 px-3 py-3",
            div { class: "min-w-0",
                div { class: "history-row-title-line min-w-0 gap-2",
                    h2 { class: "history-row-title font-semibold", "{entry.title}" }
                    span { class: "history-kind-badge px-1.5 py-0.5 font-mono text-xs font-semibold tracking-widest",
                        "{recipe_kind_label(entry.kind)}"
                    }
                }
                p { class: "history-row-range mt-1 font-mono", "{entry.range_label}" }
            }
            div { class: "min-w-0 text-ink-2",
                p { class: "truncate", "{entry.repository_name}" }
                time {
                    class: "history-row-time mt-1 font-mono tabular-nums",
                    datetime: entry.rendered_at.to_string(),
                    "{entry.rendered_at}"
                }
            }
            div { class: "history-row-actions gap-1",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    state: if opening { ButtonState::Loading } else if open_disabled { ButtonState::Disabled } else { ButtonState::Enabled },
                    aria_label: "Open {entry.title}",
                    "data-testid": test_ids::HISTORY_ENTRY_OPEN.value(),
                    onclick: move |_| onopen.call(entry.id),
                    span { aria_hidden: "true",
                        ExternalLink { size: 14 }
                    }
                    "Open"
                }
                Button {
                    size: ButtonSize::IconSmall,
                    variant: ButtonVariant::Ghost,
                    aria_label: "Copy {entry.title} JSON",
                    title: if copied { "Copied" } else { "Copy render JSON" },
                    onclick: move |_| oncopy.call(entry.id),
                    span { aria_hidden: "true",
                        if copied {
                            Check { size: 14 }
                        } else {
                            Copy { size: 14 }
                        }
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
    let Some(position) = page.position.page() else {
        return rsx! {};
    };
    let navigation = history_navigation(&page);
    let page_number = u32::from(position.number());
    let page_count = u32::from(position.count());
    let previous = navigation.previous;
    let next = navigation.next;
    let item_count_label = format!("{} renders", page.total_count);

    rsx! {
        footer { class: "history-footer min-h-14 gap-3 px-3 sm:px-4",
            p { class: "history-footer-count font-mono text-xs tabular-nums", "{item_count_label}" }
            nav {
                class: "ml-auto flex items-center gap-1",
                aria_label: "History pages",
                Button {
                    size: ButtonSize::IconMedium,
                    variant: ButtonVariant::Ghost,
                    state: if previous.is_some() { ButtonState::Enabled } else { ButtonState::Disabled },
                    aria_label: "Previous history page",
                    onclick: move |_| {
                        if let Some(cursor) = previous {
                            onnavigate.call(cursor);
                        }
                    },
                    span { aria_hidden: "true",
                        ChevronLeft { size: 15 }
                    }
                }
                output {
                    class: "history-page-number min-w-20 px-2 font-mono text-xs tabular-nums",
                    aria_label: "History page {page_number} of {page_count}",
                    "{page_number:02} / {page_count:02}"
                }
                Button {
                    size: ButtonSize::IconMedium,
                    variant: ButtonVariant::Ghost,
                    state: if next.is_some() { ButtonState::Enabled } else { ButtonState::Disabled },
                    aria_label: "Next history page",
                    onclick: move |_| {
                        if let Some(cursor) = next {
                            onnavigate.call(cursor);
                        }
                    },
                    span { aria_hidden: "true",
                        ChevronRight { size: 15 }
                    }
                }
            }
        }
    }
}
