use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::{Button, ButtonVariant, ToastHost, use_toast};

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

/// Queue, severity, and dismissal behavior.
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
            Button { onclick: move |_| toast.ok("Live view saved."), "Success" }
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
        }
    }
}

/// Toast notification component.
#[stories(id = "toast", name = "Toast", thumbnail = thumbnail)]
const TOAST_STORIES: () = &[queue];
