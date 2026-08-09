use dioxus::prelude::*;
use lucide_dioxus::TriangleAlert;

use super::{
    Button, ButtonState, ButtonVariant,
    dialog::{DialogState, sync_dialog},
};

#[component]
pub(crate) fn AlertDialog(
    id: String,
    trigger_id: String,
    open: bool,
    title: String,
    description: String,
    confirm_label: String,
    #[props(default)] confirm_state: ButtonState,
    #[props(default)] cancel_disabled: bool,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
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
    let description_id = format!("{id}-description");

    rsx! {
        dialog {
            id,
            class: "m-auto w-[min(30rem,calc(100vw-2rem))] rounded-panel border border-del-line bg-surface p-0 text-ink shadow-none backdrop:bg-bg/80",
            role: "alertdialog",
            aria_modal: "true",
            aria_labelledby: title_id.clone(),
            aria_describedby: description_id.clone(),
            oncancel: move |event| {
                event.prevent_default();
                if !cancel_disabled {
                    oncancel.call(());
                }
            },
            div { class: "flex items-start gap-3 border-b border-line px-5 py-4",
                span { class: "mt-0.5 shrink-0 text-del", aria_hidden: "true",
                    TriangleAlert { size: 18 }
                }
                div { class: "min-w-0",
                    h2 { id: title_id, class: "font-semibold text-ink", "{title}" }
                    p {
                        id: description_id,
                        class: "mt-1 leading-5 text-ink-2",
                        "{description}"
                    }
                }
            }
            div { class: "flex justify-end gap-2 px-5 py-4",
                Button {
                    variant: ButtonVariant::Ghost,
                    state: if cancel_disabled { ButtonState::Disabled } else { ButtonState::Enabled },
                    "data-dialog-initial-focus": "true",
                    onclick: move |_| oncancel.call(()),
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Destructive,
                    state: confirm_state,
                    onclick: move |_| onconfirm.call(()),
                    "{confirm_label}"
                }
            }
        }
    }
}
