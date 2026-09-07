use dioxus::prelude::*;
use dx_story::{stories, story};
use lucide_dioxus::ArrowUp;

use crate::shared::ui::{
    Button, ButtonSize, ButtonVariant,
    data_table::{DataTable, DataTableRow, TableColumn, TableHeading},
};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "grid grid-cols-3 gap-x-5 gap-y-3 font-mono text-xs",
            for label in ["Project", "Changes", "Unpushed"] {
                span { class: "border-b border-line pb-2 text-ink-3", "{label}" }
            }
            for (name, changes, count) in [("Atlas", "[!?]", "4"), ("Beacon", "[✓]", "0"), ("Cedar", "[!]", "2")] {
                span { class: "font-semibold text-ink", "{name}" }
                span { class: "text-acc", "{changes}" }
                span { class: "text-ink-2", "{count}" }
            }
        }
    }
}

/// Callers supply content and native controls within presentation columns.
#[story]
fn interactive() -> Element {
    let mut selected = use_signal(|| "No action selected".to_owned());
    rsx! {
        div { class: "w-full space-y-4",
            DataTable {
                caption: "Example projects",
                header: rsx! {
                    TableHeading { "Project" }
                    TableHeading { "Branch" }
                    TableHeading { "Changes" }
                    TableHeading {
                        span { class: "block text-right", "Unpushed" }
                    }
                    TableHeading {
                        span { class: "block text-right", "Action" }
                    }
                },
                for (name, branch, symbols, count) in [
                    ("Atlas", "main", "[!?]", 4),
                    ("Beacon", "feature/table", "[✓]", 0),
                    ("Cedar", "main", "[!]", 2),
                ]
                {
                    DataTableRow { aria_label: name,
                        TableColumn {
                            span { class: "font-semibold text-ink", "{name}" }
                        }
                        TableColumn {
                            span { class: "font-mono text-xs", "{branch}" }
                        }
                        TableColumn {
                            span { class: "font-mono text-acc", "{symbols}" }
                        }
                        TableColumn {
                            span { class: "text-right font-mono tabular-nums", "{count}" }
                        }
                        TableColumn {
                            div { class: "flex justify-end",
                                Button {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::IconMedium,
                                    aria_label: "Open {name}",
                                    title: "Open {name}",
                                    onclick: move |_| selected.set(format!("Selected {name}")),
                                    ArrowUp { size: 14 }
                                }
                            }
                        }
                    }
                }
                DataTableRow {
                    TableColumn {
                        span { class: "font-semibold text-ink", "Offline" }
                    }
                    TableColumn { colspan: 4,
                        span { class: "text-warn", "Repository unavailable" }
                    }
                }
            }
            p { role: "status", class: "text-xs text-ink-2", "{selected}" }
        }
    }
}

/// An empty table retains its headers and explains the missing content.
#[story]
fn empty() -> Element {
    rsx! {
        DataTable {
            caption: "Empty projects",
            header: rsx! {
                TableHeading { "Project" }
                TableHeading { "Status" }
            },
            DataTableRow {
                TableColumn { colspan: 2,
                    div { class: "py-10 text-center text-ink-3", "No projects to show" }
                }
            }
        }
    }
}

#[story(name = "Missing values")]
fn missing_values() -> Element {
    use crate::shared::ui::no_data::NoData;
    rsx! {
        DataTable {
            caption: "Missing data",
            header: rsx! {
                TableHeading { "Item" }
                TableHeading { "Count" }
                TableHeading { "Last updated" }
            },
            DataTableRow {
                TableColumn { "New project" }
                TableColumn { NoData {} }
                TableColumn { NoData {} }
            }
        }
    }
}

#[stories(id = "data-table", name = "DataTable", thumbnail = thumbnail)]
const DATA_TABLE_STORIES: () = &[interactive, empty, missing_values];
