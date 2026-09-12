use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

use super::ScrollArea;

#[component]
pub(crate) fn DataTable(caption: String, header: Element, children: Element) -> Element {
    rsx! {
        ScrollArea {
            class: "control-data-table min-h-0",
            role: "region",
            aria_label: caption.clone(),
            tabindex: "0",
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
