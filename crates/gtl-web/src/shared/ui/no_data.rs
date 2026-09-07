use dioxus::prelude::*;

#[component]
pub(crate) fn NoData() -> Element {
    rsx! {
        span {
            class: "font-mono text-xs font-normal text-ink-3",
            role: "img",
            aria_label: "No data",
            "-"
        }
    }
}
