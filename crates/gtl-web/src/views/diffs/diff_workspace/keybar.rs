use dioxus::prelude::*;
use gtl_contracts::viewer::ViewerFooter;

#[component]
pub(super) fn Keybar(footer: ViewerFooter) -> Element {
    rsx! {
        footer {
            class: "col-span-3 row-start-3 flex items-center gap-4 overflow-hidden border-t border-line bg-surface px-5 py-2 text-ink-3",
            aria_label: "Keyboard shortcuts",
            KeybarCommand { command: footer.command, note: footer.note }
            div { class: "flex-1" }
            KeybarShortcut { keys: vec!["j", "k"], label: "file" }
            KeybarShortcut { keys: vec!["/"], label: "filter" }
            KeybarShortcut { keys: vec!["alt+shift+c"], label: "fold all" }
        }
    }
}

#[component]
fn KeybarCommand(command: String, note: String) -> Element {
    rsx! {
        span { class: "overflow-hidden text-ellipsis whitespace-nowrap text-ink-2",
            "{command} "
            span { class: "text-ink-3", "{note}" }
        }
    }
}

#[component]
fn KeybarShortcut(keys: Vec<&'static str>, label: &'static str) -> Element {
    rsx! {
        span { class: "flex-none",
            for (index, value) in keys.into_iter().enumerate() {
                if index > 0 {
                    " "
                }
                Keycap { value }
            }
            " {label}"
        }
    }
}

#[component]
fn Keycap(value: &'static str) -> Element {
    rsx! {
        kbd { class: "rounded-sm border border-line-2 border-b-2 bg-sunk px-1.5 py-px font-mono text-ink-2",
            "{value}"
        }
    }
}
