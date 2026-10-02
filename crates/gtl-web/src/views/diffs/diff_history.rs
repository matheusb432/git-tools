pub(crate) mod tour;

use dioxus::prelude::*;
use gtl_models::viewer::RenderHistoryId;
use gtl_web_contracts::test_ids;
use gtl_wire::viewer::{
    GetViewerHistoryCopy, ListViewerHistory, OpenViewerHistory, ViewerHistoryCursor,
    ViewerHistoryEntry, ViewerHistoryFilter, ViewerHistoryPage,
};
use lucide_dioxus::{Check, Copy, ExternalLink};

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::{history_navigation, recipe_kind_label, viewer_server},
    shared::{
        browser,
        date_display::DateDisplayTime,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        recipe_label::recipe_label_text,
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, PageNotice, Select, SelectOption,
            Skeleton,
            data_table::{DataTable, DataTableRow, TableColumn, TableHeading},
            pagination::{PageNavigation, PagePosition, Pagination},
            select::SelectVariant,
        },
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
            Ok(_) => error.set(Some(ViewerClientError::Disconnected)),
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
pub(crate) fn SnapshotHistory(initial_filter: ViewerHistoryFilter) -> Element {
    let language = use_language();
    let mut filter = use_signal(move || initial_filter);
    let mut cursor = use_signal(|| ViewerHistoryCursor::Newest);
    let mut history = use_resource(move || {
        let cursor = cursor();
        let filter = filter();
        async move { viewer_server::list_history(ListViewerHistory { cursor, filter }).await }
    });
    let actions = use_history_actions();

    let pending = history.state().cloned() == UseResourceState::Pending;
    let load = history.read();
    let action_error = (actions.error)();
    let opening_id = (actions.opening_id)();
    let copied_id = (actions.copied_id)();

    let mut options = vec![
        SelectOption::new("all", t!(language, "history-filter-all")),
        SelectOption::new("unassociated", t!(language, "history-filter-unassociated")),
    ];
    if let Some(Ok(page)) = &*load {
        options.extend(
            page.projects
                .iter()
                .map(|name| SelectOption::new(format!("project:{name}"), name.to_string())),
        );
    } else if let ViewerHistoryFilter::Project { name } = filter() {
        options.push(SelectOption::new(
            format!("project:{name}"),
            name.to_string(),
        ));
    }
    let selected = match filter() {
        ViewerHistoryFilter::All => "all".to_owned(),
        ViewerHistoryFilter::Unassociated => "unassociated".to_owned(),
        ViewerHistoryFilter::Project { name } => format!("project:{name}"),
    };
    rsx! {
        div { class: "history-shell h-full min-h-0",
            div {
                class: "flex items-center gap-3 pb-4",
                "data-tour": tour::HISTORY_FILTER.value(),
                label { class: "shrink-0", r#for: "snapshot-project-filter",
                    {t!(language, "history-project")}
                }
                div { class: "min-w-0 flex-1 sm:max-w-sm",
                    Select {
                        id: "snapshot-project-filter",
                        variant: SelectVariant::Toolbar,
                        aria_label: t!(language, "history-project-label"),
                        value: selected,
                        options,
                        onchange: move |value: String| {
                            let next = match value.as_str() {
                                "all" => ViewerHistoryFilter::All,
                                "unassociated" => ViewerHistoryFilter::Unassociated,
                                value => {
                                    let Some(name) = value
                                        .strip_prefix("project:")
                                        .and_then(|name| {
                                            gtl_models::paths::ProjectName::try_new(name.to_owned()).ok()
                                        }) else {
                                        return;
                                    };
                                    ViewerHistoryFilter::Project {
                                        name,
                                    }
                                }
                            };
                            cursor.set(ViewerHistoryCursor::Newest);
                            filter.set(next);
                        },
                    }
                }
            }
            section {
                class: "history-content min-h-0",
                "data-tour": tour::HISTORY_LIST.value(),
                aria_label: t!(language, "history-renders"),
                div { class: "grid min-h-0 grid-rows-[auto_minmax(0,1fr)]",
                    div {
                        if let Some(error) = action_error {
                            div {
                                class: "history-error mb-3 px-3 py-2",
                                role: "alert",
                                {client_error_message(&error, language)}
                            }
                        }
                    }
                    match (pending, &*load) {
                        (true, _) | (false, None) => rsx! {
                            HistoryLoading {}
                        },
                        (false, Some(Err(error))) => {
                            let message = client_error_message(error, language);
                            rsx! {
                                PageNotice {
                                    class: "min-h-64",
                                    role: "alert",
                                    title: t!(language, "history-unavailable"),
                                    message,
                                    Button {
                                        class: "mx-auto mt-4",
                                        variant: ButtonVariant::Outline,
                                        onclick: move |_| history.restart(),
                                        {t!(language, "action-try-again")}
                                    }
                                }
                            }
                        }
                        (false, Some(Ok(page))) if page.entries.is_empty() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: t!(language, "history-empty"),
                                message: t!(language, "history-empty-message"),
                            }
                        },
                        (false, Some(Ok(page))) => rsx! {
                            DataTable {
                                caption: t!(language, "history-renders"),
                                header: rsx! {
                                    TableHeading { {t!(language, "history-column-id")} }
                                    TableHeading { {t!(language, "history-column-diff")} }
                                    TableHeading { {t!(language, "history-project")} }
                                    TableHeading { {t!(language, "history-column-kind")} }
                                    TableHeading { {t!(language, "history-column-range")} }
                                    TableHeading { {t!(language, "history-column-rendered")} }
                                    TableHeading { class: "text-right", {t!(language, "projects-table-actions")} }
                                },
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
                if let Some(Ok(page)) = &*load && !page.entries.is_empty() {
                    HistoryFooter {
                        page: page.clone(),
                        pending,
                        onnavigate: move |next| cursor.set(next),
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryLoading() -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "grid gap-2",
            role: "status",
            aria_label: t!(language, "history-loading"),
            for _ in 0..6 {
                div { class: "history-loading-row gap-2 p-3",
                    Skeleton { class: "h-4 w-3/5" }
                    Skeleton { class: "h-4 w-4/5" }
                    Skeleton { class: "h-8 w-24" }
                }
            }
            span { class: "sr-only", {t!(language, "history-loading")} }
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
    let language = use_language();
    let label = recipe_label_text(&entry.label, language);
    rsx! {
        DataTableRow {
            TableColumn { class: "font-mono text-xs text-ink-3", "#{entry.id}" }
            TableColumn {
                span {
                    class: "block max-w-64 truncate font-semibold text-ink",
                    title: label.clone(),
                    "{label}"
                }
            }
            TableColumn {
                span {
                    class: "block max-w-40 truncate",
                    title: "{entry.repository_name}",
                    "{entry.repository_name}"
                }
            }
            TableColumn {
                span { class: "history-kind-badge px-1.5 py-0.5 font-mono text-xs",
                    {recipe_kind_label(entry.kind, language)}
                }
            }
            TableColumn {
                span {
                    class: "block max-w-48 truncate font-mono text-xs",
                    title: entry.range_label.clone(),
                    "{entry.range_label}"
                }
            }
            TableColumn {
                DateDisplayTime {
                    class: "whitespace-nowrap font-mono text-xs text-ink-3 tabular-nums",
                    timestamp: entry.rendered_at.clone(),
                }
            }
            TableColumn {
                div {
                    class: "flex items-center justify-end gap-1",
                    "data-tour": tour::HISTORY_ACTIONS.value(),
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Outline,
                        state: if opening { ButtonState::Loading } else if open_disabled { ButtonState::Disabled } else { ButtonState::Enabled },
                        aria_label: t!(language, "history-open-named", title = label.as_str()),
                        "data-testid": test_ids::HISTORY_ENTRY_OPEN.value(),
                        onclick: move |_| onopen.call(entry.id),
                        span { aria_hidden: "true",
                            ExternalLink { size: 14 }
                        }
                        {t!(language, "history-open")}
                    }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: t!(language, "history-copy-json-named", title = label.as_str()),
                        title: if copied { t!(language, "copy-copied") } else { t!(language, "history-copy-json") },
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
}

#[component]
fn HistoryFooter(
    page: ViewerHistoryPage,
    pending: bool,
    onnavigate: EventHandler<ViewerHistoryCursor>,
) -> Element {
    let language = use_language();
    let Some(position) = page.position.page() else {
        return rsx! {};
    };
    let navigation = history_navigation(&page);
    let page_number = u32::from(position.number());
    let page_count = u32::from(position.count());
    let previous = navigation.previous;
    let next = navigation.next;
    rsx! {
        Pagination {
            position: PagePosition::new(page_number as usize, page_count as usize),
            label: t!(language, "history-label"),
            disabled: pending,
            onselect: move |navigation| {
                let next = match navigation {
                    PageNavigation::First => Some(ViewerHistoryCursor::Newest),
                    PageNavigation::Previous => previous,
                    PageNavigation::Next => next,
                    PageNavigation::Last => Some(ViewerHistoryCursor::Oldest),
                };
                if let Some(next) = next {
                    onnavigate.call(next);
                }
            },
            span { class: "whitespace-nowrap",
                {t!(language, "history-render-count", count = page.total_count.into_inner())}
            }
        }
    }
}
