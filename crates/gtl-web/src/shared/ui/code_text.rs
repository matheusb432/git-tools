use dioxus::prelude::*;

#[component]
pub fn CodeText(children: Element) -> Element {
    rsx! {
        code { class: "rounded-sm border border-line-2 bg-sunk px-1.5 py-0.5 font-mono text-acc",
            {children}
        }
    }
}
