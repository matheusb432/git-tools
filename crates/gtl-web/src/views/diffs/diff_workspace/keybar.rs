use dioxus::prelude::*;
use gtl_contracts::viewer::ViewerFooter;

const KEY_CLASSES: &str =
    "rounded-sm border border-line-2 border-b-2 bg-sunk px-1.5 py-px font-mono text-ink-2";

#[component]
pub(super) fn Keybar(footer: ViewerFooter) -> Element {
    rsx! {
        footer {
            class: "col-span-3 row-start-3 flex items-center gap-4 overflow-hidden border-t border-line bg-surface px-5 py-2 text-ink-3",
            aria_label: "Keyboard shortcuts",
            span { class: "overflow-hidden text-ellipsis whitespace-nowrap text-ink-2",
                "{footer.command} "
                span { class: "text-ink-3", "{footer.note}" }
            }
            div { class: "flex-1" }
            span { class: "flex-none",
                kbd { class: KEY_CLASSES, "j" }
                " "
                kbd { class: KEY_CLASSES, "k" }
                " file"
            }
            span { class: "flex-none",
                kbd { class: KEY_CLASSES, "/" }
                " filter"
            }
            span { class: "flex-none",
                kbd { class: KEY_CLASSES, "alt+shift+c" }
                " fold all"
            }
        }
    }
}
