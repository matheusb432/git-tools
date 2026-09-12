use dioxus::prelude::*;
use lucide_dioxus::X;

use super::{Button, ButtonSize, ButtonVariant, ScrollArea, dialog::use_dialog};

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum PanelDialogVariant {
    #[default]
    Panel,
    #[cfg(feature = "desktop")]
    Table,
}

#[component]
pub(crate) fn PanelDialog(
    id: String,
    trigger_id: String,
    open: bool,
    title: String,
    #[props(default)] variant: PanelDialogVariant,
    onclose: EventHandler<()>,
    artifact_view_id: Option<String>,
    children: Element,
) -> Element {
    use_dialog(&id, &trigger_id, open);
    let title_id = format!("{id}-title");
    let artifact_dialog = artifact_view_id.as_ref().map(|_| "");
    let artifact_close_action = artifact_view_id.as_ref().map(|_| "close-dialog");
    let artifact_trigger_id = artifact_view_id.as_ref().map(|_| trigger_id.clone());
    let class = match variant {
        PanelDialogVariant::Panel => "dialog-surface m-auto p-0",
        #[cfg(feature = "desktop")]
        PanelDialogVariant::Table => "dialog-surface m-auto w-[min(72rem,calc(100vw-2rem))] p-0",
    };

    rsx! {
        dialog {
            id,
            class,
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
            div { class: "dialog-body h-full min-h-0",
                header { class: "dialog-header gap-3 px-4 py-3",
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
                match variant {
                    PanelDialogVariant::Panel => rsx! {
                        ScrollArea { class: "min-h-0 overflow-auto p-4",
                            if open || artifact_view_id.is_some() {
                                {children}
                            }
                        }
                    },
                    #[cfg(feature = "desktop")]
                    PanelDialogVariant::Table => rsx! {
                        div { class: "min-h-0 overflow-hidden p-4",
                            if open {
                                {children}
                            }
                        }
                    },
                }
            }
        }
    }
}
