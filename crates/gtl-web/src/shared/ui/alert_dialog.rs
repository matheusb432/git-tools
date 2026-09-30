use std::time::Duration;

use dioxus::prelude::*;
use lucide_dioxus::{CircleX, Info, TriangleAlert, X};

use super::{
    Button, ButtonSize, ButtonState, ButtonVariant, SectionedSurface, SectionedSurfaceBody,
    SectionedSurfaceFooter, SectionedSurfaceHeader,
    dialog::{DialogPlacement, use_dialog},
};
use crate::shared::i18n::{t, use_language};

const ALERT_DIALOG_CLOSE_DURATION: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AlertDialogVariant {
    #[default]
    Alert,
    Info,
    Error,
}

impl AlertDialogVariant {
    const fn value(self) -> &'static str {
        match self {
            Self::Alert => "alert",
            Self::Info => "info",
            Self::Error => "error",
        }
    }

    const fn confirm_button_variant(self) -> ButtonVariant {
        match self {
            Self::Alert => ButtonVariant::Warning,
            Self::Info => ButtonVariant::Primary,
            Self::Error => ButtonVariant::Destructive,
        }
    }
}

#[component]
pub(crate) fn AlertDialog(
    id: String,
    trigger_id: String,
    open: bool,
    title: String,
    description: String,
    confirm_label: String,
    #[props(default)] variant: AlertDialogVariant,
    #[props(default)] confirm_state: ButtonState,
    #[props(default)] cancel_disabled: bool,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
    children: Option<Element>,
) -> Element {
    let language = use_language();
    let phase = use_dialog(
        &id,
        &trigger_id,
        open,
        ALERT_DIALOG_CLOSE_DURATION,
        DialogPlacement::Center,
    );

    let title_id = format!("{id}-title");
    let description_id = format!("{id}-description");

    rsx! {
        dialog {
            id,
            class: "alert-dialog",
            role: "alertdialog",
            aria_modal: "true",
            aria_labelledby: title_id.clone(),
            aria_describedby: description_id.clone(),
            "data-state": phase().value(),
            "data-variant": variant.value(),
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    if !cancel_disabled {
                        oncancel.call(());
                    }
                }
            },
            oncancel: move |event| {
                event.prevent_default();
                if !cancel_disabled {
                    oncancel.call(());
                }
            },
            SectionedSurface { class: "alert-dialog-surface", "data-dialog-surface": "true",
                SectionedSurfaceHeader { class: "alert-dialog-header",
                    span { class: "alert-dialog-icon", aria_hidden: "true",
                        match variant {
                            AlertDialogVariant::Alert => rsx! {
                                TriangleAlert { size: 20 }
                            },
                            AlertDialogVariant::Info => rsx! {
                                Info { size: 20 }
                            },
                            AlertDialogVariant::Error => rsx! {
                                CircleX { size: 20 }
                            },
                        }
                    }
                    h2 { id: title_id.clone(), class: "alert-dialog-title", "{title}" }
                    Button {
                        class: "alert-dialog-close",
                        size: ButtonSize::IconTouch,
                        variant: ButtonVariant::Ghost,
                        state: if cancel_disabled { ButtonState::Disabled } else { ButtonState::Enabled },
                        aria_label: t!(language, "dialog-close"),
                        title: t!(language, "dialog-close"),
                        onclick: move |_| oncancel.call(()),
                        span { aria_hidden: "true",
                            X { size: 20 }
                        }
                    }
                }
                SectionedSurfaceBody { class: "alert-dialog-body",
                    p {
                        id: description_id.clone(),
                        class: "alert-dialog-description",
                        "{description}"
                    }
                    if let Some(children) = children {
                        div { class: "alert-dialog-content", {children} }
                    }
                }
                SectionedSurfaceFooter { class: "alert-dialog-actions",
                    Button {
                        size: ButtonSize::Medium,
                        variant: ButtonVariant::Ghost,
                        state: if cancel_disabled { ButtonState::Disabled } else { ButtonState::Enabled },
                        "data-dialog-initial-focus": "true",
                        onclick: move |_| oncancel.call(()),
                        {t!(language, "dialog-cancel")}
                    }
                    Button {
                        size: ButtonSize::Medium,
                        variant: variant.confirm_button_variant(),
                        state: confirm_state,
                        onclick: move |_| onconfirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AlertDialogVariant, ButtonVariant};

    #[test]
    fn semantic_variants_choose_matching_confirmation_buttons() {
        assert_eq!(
            AlertDialogVariant::Alert.confirm_button_variant(),
            ButtonVariant::Warning
        );
        assert_eq!(
            AlertDialogVariant::Info.confirm_button_variant(),
            ButtonVariant::Primary
        );
        assert_eq!(
            AlertDialogVariant::Error.confirm_button_variant(),
            ButtonVariant::Destructive
        );
    }
}
