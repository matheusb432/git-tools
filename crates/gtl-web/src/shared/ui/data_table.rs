use dioxus::prelude::*;

#[component]
pub(crate) fn DataTable(caption: String, header: Element, children: Element) -> Element {
    rsx! {
        div {
            class: "overflow-x-auto rounded-panel border border-line bg-surface focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc",
            role: "region",
            aria_label: caption.clone(),
            tabindex: "0",
            table { class: "w-full min-w-[44rem] border-collapse text-left text-sm",
                caption { class: "sr-only", "{caption}" }
                thead { class: "border-b border-line-2 bg-sunk font-mono text-[10px] tracking-wider whitespace-nowrap text-ink-3 uppercase",
                    tr { {header} }
                }
                tbody { {children} }
            }
        }
    }
}

#[component]
pub(crate) fn TableHeading(children: Element) -> Element {
    rsx! {
        th { scope: "col", class: "px-4 py-3 font-medium", {children} }
    }
}

#[component]
pub(crate) fn TableColumn(#[props(default = 1)] colspan: u32, children: Element) -> Element {
    rsx! {
        td { colspan, class: "h-px px-4 py-3 align-middle", {children} }
    }
}

#[component]
pub(crate) fn DataTableRow(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        tr {
            class: "border-b border-line text-ink-2 last:border-b-0 even:bg-sunk/30 hover:bg-surface-2 focus-within:bg-surface-2",
            ..attributes,
            {children}
        }
    }
}
