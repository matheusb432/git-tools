use dioxus::prelude::*;
use lucide_dioxus::X;

use super::{
    Button, ButtonSize, ButtonVariant,
    dialog::{DialogState, sync_dialog},
};

#[component]
pub(crate) fn Popover(
    id: String,
    trigger_id: String,
    open: bool,
    title: String,
    onclose: EventHandler<()>,
    children: Element,
) -> Element {
    let mut was_open = use_signal(|| false);
    use_effect(use_reactive(
        (&id, &trigger_id, &open),
        move |(id, trigger_id, open)| {
            let restore_focus = *was_open.peek() && !open;
            was_open.set(open);
            sync_dialog(DialogState {
                id,
                trigger_id,
                open,
                restore_focus,
            });
        },
    ));
    let title_id = format!("{id}-title");

    rsx! {
        dialog {
            id,
            class: "m-auto h-[min(42rem,calc(100vh-2rem))] w-[min(34rem,calc(100vw-2rem))] rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-none backdrop:bg-bg/80",
            aria_modal: "true",
            aria_labelledby: title_id.clone(),
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    onclose.call(());
                }
            },
            div { class: "grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)]",
                header { class: "flex items-center justify-between gap-3 border-b border-line px-4 py-3",
                    h2 { id: title_id, class: "text-sm font-semibold text-ink", "{title}" }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: "Close {title}",
                        title: "Close",
                        "data-dialog-initial-focus": "true",
                        onclick: move |_| onclose.call(()),
                        span { aria_hidden: "true",
                            X { size: 15 }
                        }
                    }
                }
                div { class: "min-h-0 overflow-auto p-4 [scrollbar-color:var(--color-line-2)_transparent] [scrollbar-width:thin]",
                    {children}
                }
            }
        }
    }
}
