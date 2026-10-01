use std::time::Duration;

use dioxus::prelude::*;
use lucide_dioxus::X;

use super::{
    Button, ButtonSize, ButtonVariant, ScrollArea,
    dialog::{DialogPhase, DialogPlacement, use_dialog},
};
use crate::shared::i18n::{t, use_language};

const PANEL_DIALOG_CLOSE_DURATION: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum PanelDialogVariant {
    #[default]
    Panel,
    Table,
    /// Fits a short form instead of filling the panel height.
    Form,
}

#[component]
pub(crate) fn PanelDialog(
    id: String,
    trigger_id: String,
    open: bool,
    title: String,
    #[props(default)] variant: PanelDialogVariant,
    onclose: EventHandler<()>,
    onclosed: Option<EventHandler<()>>,
    children: Element,
) -> Element {
    let language = use_language();
    let phase = use_dialog(
        &id,
        &trigger_id,
        open,
        PANEL_DIALOG_CLOSE_DURATION,
        DialogPlacement::Center,
        onclosed,
    );
    let rendered = open || phase() != DialogPhase::Closed;
    let title_id = format!("{id}-title");
    let class = match variant {
        PanelDialogVariant::Panel => "dialog-surface m-auto p-0",
        PanelDialogVariant::Table => "dialog-surface m-auto w-[min(72rem,calc(100vw-2rem))] p-0",
        PanelDialogVariant::Form => "dialog-surface dialog-surface-fit m-auto p-0",
    };
    let body_class = match variant {
        PanelDialogVariant::Form => "dialog-body dialog-body-fit min-h-0",
        PanelDialogVariant::Panel | PanelDialogVariant::Table => "dialog-body h-full min-h-0",
    };

    rsx! {
        dialog {
            id,
            class,
            aria_modal: "true",
            aria_labelledby: title_id.clone(),
            "data-state": phase().value(),
            "data-dialog-surface": "true",
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    onclose.call(());
                }
            },
            div { class: body_class,
                header { class: "dialog-header gap-3 px-4 py-3",
                    h2 { id: title_id, class: "font-semibold text-ink", "{title}" }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: t!(language, "dialog-close-named", title = title.as_str()),
                        title: t!(language, "dialog-close-short"),
                        "data-dialog-initial-focus": "true",

                        onclick: move |_| onclose.call(()),
                        span { aria_hidden: "true",
                            X { size: 15 }
                        }
                    }
                }
                match variant {
                    PanelDialogVariant::Panel | PanelDialogVariant::Form => rsx! {
                        ScrollArea { class: "min-h-0 overflow-auto p-4",
                            if rendered {
                                {children}
                            }
                        }
                    },
                    PanelDialogVariant::Table => rsx! {
                        div { class: "min-h-0 overflow-hidden p-4",
                            if rendered {
                                {children}
                            }
                        }
                    },
                }
            }
        }
    }
}
