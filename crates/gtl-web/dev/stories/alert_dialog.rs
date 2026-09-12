use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::{AlertDialog, Button, ButtonState, ButtonVariant};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: "preview-alert-preview-trigger",
                variant: ButtonVariant::Destructive,
                "Delete item"
            }
            AlertDialog {
                id: "preview-alert-preview-dialog",
                trigger_id: "preview-alert-preview-trigger",
                open: false,
                title: "Delete item",
                description: "This action cannot be undone.",
                confirm_label: "Delete item",
                oncancel: move |()| {},
                onconfirm: move |()| {},
            }
        }
    }
}

/// Open, cancel, confirm, and restore focus.
#[story]
fn interactive() -> Element {
    let mut open = use_signal(|| false);
    let mut outcome = use_signal(|| "No action selected");

    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: "preview-alert-trigger",
                variant: ButtonVariant::Destructive,
                onclick: move |_| open.set(true),
                "Delete item"
            }
            output { class: "text-sm text-ink-2", aria_live: "polite", "{outcome}" }
            AlertDialog {
                id: "preview-alert-dialog",
                trigger_id: "preview-alert-trigger",
                open: open(),
                title: "Delete item",
                description: "This action cannot be undone.",
                confirm_label: "Delete item",
                oncancel: move |()| {
                    open.set(false);
                    outcome.set("Canceled");
                },
                onconfirm: move |()| {
                    open.set(false);
                    outcome.set("Deleted");
                },
            }
        }
    }
}

/// Disabled actions during confirmation.
#[story(name = "Pending confirmation")]
fn pending() -> Element {
    let mut open = use_signal(|| false);

    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: "preview-pending-alert-trigger",
                variant: ButtonVariant::Destructive,
                onclick: move |_| open.set(true),
                "Open pending state"
            }
            AlertDialog {
                id: "preview-pending-alert-dialog",
                trigger_id: "preview-pending-alert-trigger",
                open: open(),
                title: "Delete item",
                description: "The item is being deleted.",
                confirm_label: "Deleting",
                confirm_state: ButtonState::Loading,
                cancel_disabled: true,
                oncancel: move |()| {},
                onconfirm: move |()| {},
            }
        }
    }
}

/// Confirmation dialog component.
#[stories(id = "alert-dialog", name = "Alert dialog", thumbnail = thumbnail)]
const ALERT_DIALOG_STORIES: () = &[interactive, pending];
