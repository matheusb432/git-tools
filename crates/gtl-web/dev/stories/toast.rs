use dioxus::prelude::*;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::{Button, ButtonVariant, ToastHost, use_toast};

pub(super) const VARIANTS: &[StoryVariant] = &[StoryVariant {
    slug: "queue",
    title: "Interactive queue",
    summary: "Queue, severity, and dismissal.",
    render: queue,
}];

pub(super) fn preview(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        ToastHost { key: "{generation}", ToastPreviewControl {} }
    }
}

#[component]
fn ToastPreviewControl() -> Element {
    let toast = use_toast();

    rsx! {
        Button { onclick: move |_| toast.ok("Changes saved."), "Show toast" }
    }
}

fn queue(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        ToastHost { key: "{generation}", ToastControls {} }
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
