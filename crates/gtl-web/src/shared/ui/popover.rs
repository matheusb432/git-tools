use dioxus::prelude::*;
use lucide_dioxus::X;

use super::{Button, ButtonSize, ButtonVariant, ScrollArea, dialog::use_dialog};

#[component]
pub(crate) fn Popover(
    id: String,
    trigger_id: String,
    open: bool,
    title: String,
    onclose: EventHandler<()>,
    artifact_view_id: Option<String>,
    children: Element,
) -> Element {
    use_dialog(&id, &trigger_id, open);
    let title_id = format!("{id}-title");
    let artifact_dialog = artifact_view_id.as_ref().map(|_| "");
    let artifact_close_action = artifact_view_id.as_ref().map(|_| "close-dialog");
    let artifact_trigger_id = artifact_view_id.as_ref().map(|_| trigger_id.clone());

    rsx! {
        dialog {
            id,
            class: "m-auto h-[min(42rem,calc(100vh-2rem))] w-[min(34rem,calc(100vw-2rem))] rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-none backdrop:bg-bg/80",
            aria_modal: "true",
            aria_labelledby: title_id.clone(),
            "data-gtl-dialog": artifact_dialog,
            "data-gtl-dialog-trigger": artifact_trigger_id,
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    onclose.call(());
                }
            },
            div { class: "grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)]",
                header { class: "flex items-center justify-between gap-3 border-b border-line px-4 py-3",
                    h2 { id: title_id, class: "font-semibold text-ink", "{title}" }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: "Close {title}",
                        title: "Close",
                        "data-dialog-initial-focus": "true",
                        "data-gtl-action": artifact_close_action,
                        onclick: move |_| onclose.call(()),
                        span { aria_hidden: "true",
                            X { size: 15 }
                        }
                    }
                }
                ScrollArea { class: "min-h-0 overflow-auto p-4", {children} }
            }
        }
    }
}
