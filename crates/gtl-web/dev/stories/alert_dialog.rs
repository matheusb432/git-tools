use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::{AlertDialog, AlertDialogVariant, Button, ButtonState, ButtonVariant};

const PUSH_COMMIT_SHA: &str = "a101a101a101a101a101a101a101a101a101a101";

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: "preview-alert-preview-trigger",
                variant: ButtonVariant::Outline,
                "Review warning"
            }
            AlertDialog {
                id: "preview-alert-preview-dialog",
                trigger_id: "preview-alert-preview-trigger",
                open: false,
                title: "Overwrite snapshot",
                description: "The existing snapshot will be replaced.",
                confirm_label: "Overwrite",
                variant: AlertDialogVariant::Alert,
                oncancel: move |()| {},
                onconfirm: move |()| {},
            }
        }
    }
}

/// Warning confirmation with a non-destructive primary action.
#[story(name = "Alert")]
fn interactive() -> Element {
    rsx! {
        AlertDialogStory {
            id: "preview-alert",
            trigger_label: "Overwrite snapshot",
            trigger_variant: ButtonVariant::Outline,
            variant: AlertDialogVariant::Alert,
            title: "Overwrite snapshot?",
            description: "The existing snapshot will be replaced with the current comparison.",
            confirm_label: "Overwrite",
            confirmed_outcome: "Snapshot overwritten",
        }
    }
}

/// Informational confirmation for a reversible navigation choice.
#[story]
fn info() -> Element {
    rsx! {
        AlertDialogStory {
            id: "preview-info",
            trigger_label: "Open comparison",
            trigger_variant: ButtonVariant::Primary,
            variant: AlertDialogVariant::Info,
            title: "Open comparison?",
            description: "The comparison will open in a new viewer tab.",
            confirm_label: "Open comparison",
            confirmed_outcome: "Comparison opened",
        }
    }
}

/// Error confirmation with a destructive action.
#[story]
fn error() -> Element {
    rsx! {
        AlertDialogStory {
            id: "preview-error",
            trigger_label: "Delete snapshot",
            trigger_variant: ButtonVariant::Destructive,
            variant: AlertDialogVariant::Error,
            title: "Delete saved snapshot?",
            description: "This permanently removes the snapshot from render history.",
            confirm_label: "Delete snapshot",
            confirmed_outcome: "Snapshot deleted",
        }
    }
}

/// Compact push review beside the production review dock.
#[story(name = "Push confirmation")]
fn push_confirmation() -> Element {
    let mut open = use_signal(|| false);
    let mut outcome = use_signal(|| "No action selected");
    let preview = gtl_wire::viewer::push::ViewerPushPreview {
        no_confirmation: false,
        repository: std::path::PathBuf::from("/repos/git-tools").try_into()?,
        project: Some("git-tools".to_owned().try_into()?),
        branch: "main".to_owned().try_into()?,
        remote_branch: "main".to_owned().try_into()?,
        remote: "origin".to_owned().try_into()?,
        remote_url: "git@github.com:example/git-tools.git"
            .to_owned()
            .try_into()?,
        commit: PUSH_COMMIT_SHA.parse()?,
        count: 2,
        command: format!(
            "git -C /repos/git-tools -c remote.origin.mirror=false push --atomic --porcelain --no-follow-tags --recurse-submodules=no -- origin {PUSH_COMMIT_SHA}:refs/heads/main",
        ),
        command_arguments: vec![
            gtl_wire::viewer::push::ViewerPushCommandArgument::Git,
            gtl_wire::viewer::push::ViewerPushCommandArgument::WorkingDirectory,
            gtl_wire::viewer::push::ViewerPushCommandArgument::DisableMirroring,
            gtl_wire::viewer::push::ViewerPushCommandArgument::Push,
            gtl_wire::viewer::push::ViewerPushCommandArgument::Atomic,
            gtl_wire::viewer::push::ViewerPushCommandArgument::Porcelain,
            gtl_wire::viewer::push::ViewerPushCommandArgument::NoFollowTags,
            gtl_wire::viewer::push::ViewerPushCommandArgument::NoRecurseSubmodules,
            gtl_wire::viewer::push::ViewerPushCommandArgument::OptionSeparator,
            gtl_wire::viewer::push::ViewerPushCommandArgument::Remote,
            gtl_wire::viewer::push::ViewerPushCommandArgument::CommitRef,
        ],
    };
    rsx! {
        div { class: "grid gap-3",
            div { class: "relative h-96 w-[min(48rem,calc(100vw-2rem))] overflow-hidden rounded-panel border border-line bg-bg",
                pre { class: "p-5 text-sm leading-7 text-ink-2",
                    "src/review.rs\n\n+ review_snapshot();\n+ approve_changes();\n+ continue_review();"
                }
                crate::views::diffs::diff_workspace::review_actions::ReviewActionDock {
                    unpushed: Some(true),
                    close_disabled: false,
                    onclose: move |_| {},
                    push: rsx! {
                        Button {
                            id: "preview-push-trigger",
                            class: "review-push-action",
                            variant: ButtonVariant::Accent,
                            aria_label: "Review push",
                            icon: rsx! {
                                lucide_dioxus::Upload { size: 14 }
                            },
                            onclick: move |_| open.set(true),
                            "Push"
                        }
                    },
                }
            }
            output { class: "text-sm text-ink-2", aria_live: "polite", "{outcome}" }
            crate::views::push::confirmation::PushConfirmationDialog {
                id: "preview-push-dialog",
                trigger_id: "preview-push-trigger",
                open: open(),
                preview,
                oncancel: move |()| {
                    open.set(false);
                    outcome.set("Canceled");
                },
                onconfirm: move |()| {
                    open.set(false);
                    outcome.set("Push confirmed");
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
                title: "Delete saved snapshot?",
                description: "The snapshot is being deleted.",
                confirm_label: "Deleting",
                variant: AlertDialogVariant::Error,
                confirm_state: ButtonState::Loading,
                cancel_disabled: true,
                oncancel: move |()| {},
                onconfirm: move |()| {},
            }
        }
    }
}

#[component]
fn AlertDialogStory(
    id: String,
    trigger_label: String,
    trigger_variant: ButtonVariant,
    variant: AlertDialogVariant,
    title: String,
    description: String,
    confirm_label: String,
    confirmed_outcome: String,
    children: Option<Element>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut outcome = use_signal(|| "No action selected".to_owned());
    let trigger_id = format!("{id}-trigger");
    let dialog_id = format!("{id}-dialog");
    let canceled_outcome = "Canceled".to_owned();

    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                id: trigger_id.clone(),
                variant: trigger_variant,
                onclick: move |_| open.set(true),
                "{trigger_label}"
            }
            output { class: "text-sm text-ink-2", aria_live: "polite", "{outcome}" }
            AlertDialog {
                id: dialog_id,
                trigger_id,
                open: open(),
                title,
                description,
                confirm_label,
                variant,
                oncancel: move |()| {
                    open.set(false);
                    outcome.set(canceled_outcome.clone());
                },
                onconfirm: move |()| {
                    open.set(false);
                    outcome.set(confirmed_outcome.clone());
                },
                {children}
            }
        }
    }
}

/// Semantic confirmation dialog variants and the GTL-0081 push review.
#[stories(id = "alert-dialog", name = "Alert dialog", thumbnail = thumbnail)]
const ALERT_DIALOG_STORIES: () = &[interactive, info, error, push_confirmation, pending];
