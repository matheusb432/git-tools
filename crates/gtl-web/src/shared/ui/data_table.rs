use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use lucide_dioxus::{ArrowDown, ArrowUp, ArrowUpDown};

use super::ScrollArea;
use crate::shared::i18n::{t, use_language};

#[component]
pub(crate) fn DataTable(
    caption: String,
    header: Element,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: "control-data-table min-h-0",
        role: "region",
        aria_label: caption.clone(),
        tabindex: "0",
    });
    let attributes = merge_attributes(vec![attributes, base]);
    rsx! {
        ScrollArea { attributes,
            table { class: "control-data-table-table w-full text-sm",
                caption { class: "sr-only", "{caption}" }
                thead { class: "control-data-table-head text-xs",
                    tr { {header} }
                }
                tbody { {children} }
            }
        }
    }
}

#[component]
pub(crate) fn DataTableActions(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "control-table-actions", ..attributes, {children} }
    }
}

#[component]
pub(crate) fn TableHeading(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(th {
        scope: "col",
        class: "h-9 px-3 font-medium",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        th { ..attributes,{children} }
    }
}

#[component]
pub(crate) fn TableSortHeading(
    label: String,
    direction: Option<TableSortDirection>,
    #[props(default)] disabled: bool,
    onsort: EventHandler<()>,
) -> Element {
    let language = use_language();
    let column = label.as_str();
    let (aria_sort, title) = match direction {
        Some(TableSortDirection::Ascending) => (
            Some("ascending"),
            t!(language, "table-sort-descending", column = column),
        ),
        Some(TableSortDirection::Descending) => (
            Some("descending"),
            t!(language, "table-sort-ascending", column = column),
        ),
        None => (None, t!(language, "table-sort-by", column = column)),
    };
    rsx! {
        TableHeading { aria_sort,
            button {
                r#type: "button",
                class: "control-table-sort",
                title,
                disabled,
                onclick: move |_| onsort(()),
                "{label}"
                span { class: "inline-flex shrink-0", aria_hidden: "true",
                    match direction {
                        Some(TableSortDirection::Ascending) => rsx! {
                            ArrowUp { size: 13 }
                        },
                        Some(TableSortDirection::Descending) => rsx! {
                            ArrowDown { size: 13 }
                        },
                        None => rsx! {
                            ArrowUpDown { size: 13 }
                        },
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TableSortDirection {
    Ascending,
    Descending,
}

#[component]
pub(crate) fn TableColumn(
    #[props(default = 1)] colspan: u32,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(td {
        colspan,
        class: "control-table-column h-11 px-3",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        td { ..attributes,{children} }
    }
}

#[component]
pub(crate) fn DataTableRow(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        tr { class: "control-data-table-row", ..attributes, {children} }
    }
}
