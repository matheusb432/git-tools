use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::{Button, ButtonVariant, ToastHost, ToastKind, use_toast};

const PUSH_REJECTED_DETAIL: &str = "hint: Updates were rejected because the remote contains work that you do not\n\
    hint: have locally. This is usually caused by another repository pushing to\n\
    hint: the same ref. If you want to integrate the remote changes, use\n\
    hint: 'git pull' before pushing again.";

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        ToastHost { PreviewToastControl {} }
    }
}

#[component]
fn PreviewToastControl() -> Element {
    let toast = use_toast();

    rsx! {
        Button { onclick: move |_| toast.ok("Saved."), "Show toast" }
    }
}

/// Queue, severity, pause, and exit behavior.
#[story(name = "Interactive queue")]
fn queue() -> Element {
    rsx! {
        ToastHost { ToastControls {} }
    }
}

#[component]
fn ToastControls() -> Element {
    let toast = use_toast();

    rsx! {
        div { class: "flex flex-wrap gap-3",
            Button { onclick: move |_| toast.ok("Tab updated."), "Success" }
            Button {
                variant: ButtonVariant::Secondary,
                onclick: move |_| toast.info("Rendering continues in the background."),
                "Information"
            }
            Button {
                variant: ButtonVariant::Outline,
                onclick: move |_| toast.warn("The working tree changed."),
                "Warning"
            }
            Button {
                variant: ButtonVariant::Failure,
                onclick: move |_| toast.error("The local viewer is unavailable."),
                "Error"
            }
            Button {
                variant: ButtonVariant::Outline,
                onclick: move |_| {
                    toast.ok("Push completed.");
                    toast.warn("The working tree changed.");
                    toast.error("The local viewer is unavailable.");
                },
                "Queue three"
            }
            Button {
                variant: ButtonVariant::Outline,
                onclick: move |_| {
                    toast
                        .show(
                            ToastKind::Error,
                            "The remote has commits that this branch does not. Pull them before pushing.",
                            Some(PUSH_REJECTED_DETAIL.to_owned()),
                        );
                },
                "With detail"
            }
        }
    }
}

/// Toast notification component.
#[stories(id = "toast", name = "Toast", thumbnail = thumbnail)]
const TOAST_STORIES: () = &[queue];
