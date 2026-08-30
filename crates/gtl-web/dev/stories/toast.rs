use dioxus::prelude::*;
use dx_book::{story, variant};

use crate::shared::ui::{Button, ButtonVariant, ToastHost, use_toast};

#[variant(name = "Catalog preview")]
fn preview() -> Element {
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
#[variant(name = "Interactive queue")]
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
#[story(id = "toast", name = "Toast", preview = preview)]
const TOAST_STORY: () = &[queue];
