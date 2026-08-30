use dioxus::prelude::*;
use dx_book::{story, variant};

use crate::shared::ui::{AlertDialog, Button, ButtonState, ButtonVariant};

#[variant(name = "Catalog preview")]
fn preview() -> Element {
    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: "story-alert-preview-trigger",
                variant: ButtonVariant::Destructive,
                "Delete item"
            }
            AlertDialog {
                id: "story-alert-preview-dialog",
                trigger_id: "story-alert-preview-trigger",
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
#[variant]
fn interactive() -> Element {
    let mut open = use_signal(|| false);
    let mut outcome = use_signal(|| "No action selected");

    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: "story-alert-trigger",
                variant: ButtonVariant::Destructive,
                onclick: move |_| open.set(true),
                "Delete live view"
            }
            output { class: "text-sm text-ink-2", aria_live: "polite", "{outcome}" }
            AlertDialog {
                id: "story-alert-dialog",
                trigger_id: "story-alert-trigger",
                open: open(),
                title: "Delete live view",
                description: "This removes the saved live view. Render history remains available.",
                confirm_label: "Delete live view",
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
#[variant(name = "Pending confirmation")]
fn pending() -> Element {
    let mut open = use_signal(|| false);

    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: "story-pending-alert-trigger",
                variant: ButtonVariant::Destructive,
                onclick: move |_| open.set(true),
                "Open pending state"
            }
            AlertDialog {
                id: "story-pending-alert-dialog",
                trigger_id: "story-pending-alert-trigger",
                open: open(),
                title: "Delete live view",
                description: "The viewer is waiting for confirmation from the local service.",
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
#[story(id = "alert-dialog", name = "Alert dialog", preview = preview)]
const ALERT_DIALOG_STORY: () = &[interactive, pending];
