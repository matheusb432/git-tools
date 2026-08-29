use dioxus::prelude::*;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::{AlertDialog, Button, ButtonState, ButtonVariant};

pub(super) const VARIANTS: &[StoryVariant] = &[
    StoryVariant {
        slug: "interactive",
        title: "Interactive",
        summary: "Open, cancel, confirm, and restore focus.",
        render: interactive,
    },
    StoryVariant {
        slug: "pending",
        title: "Pending confirmation",
        summary: "Disabled actions during confirmation.",
        render: pending,
    },
];

pub(super) fn preview(_context: StoryContext) -> Element {
    rsx! {
        div { class: "grid justify-items-center",
            Button {
                id: "story-preview-alert-trigger",
                variant: ButtonVariant::Destructive,
                "Delete item"
            }
            AlertDialog {
                id: "story-preview-alert-dialog",
                trigger_id: "story-preview-alert-trigger",
                open: false,
                title: "Delete item?",
                description: "This action cannot be undone.",
                confirm_label: "Delete",
                oncancel: move |()| {},
                onconfirm: move |()| {},
            }
        }
    }
}

fn interactive(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        InteractiveAlertDialog { key: "{generation}" }
    }
}

#[component]
fn InteractiveAlertDialog() -> Element {
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

fn pending(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        PendingAlertDialog { key: "{generation}" }
    }
}

#[component]
fn PendingAlertDialog() -> Element {
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
