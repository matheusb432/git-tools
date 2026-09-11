use dioxus::prelude::*;
use lucide_dioxus::TriangleAlert;

use super::{Button, ButtonState, ButtonVariant, dialog::use_dialog};

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
    use_dialog(&id, &trigger_id, open);

    let title_id = format!("{id}-title");
    let description_id = format!("{id}-description");

    rsx! {
        dialog {
            id,
            class: "alert-dialog-surface m-auto p-0",
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
            div { class: "alert-dialog-header gap-3 px-5 py-4",
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
            div { class: "alert-dialog-actions gap-2 px-5 py-4",
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
