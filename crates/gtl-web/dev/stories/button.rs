use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::{Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "grid grid-cols-2 gap-3",
            Button { variant: ButtonVariant::Primary, "Primary" }
            Button { variant: ButtonVariant::Secondary, "Secondary" }
            Button { variant: ButtonVariant::Ghost, "Ghost" }
            Button { state: ButtonState::Disabled, "Disabled" }
        }
    }
}

/// All visual variants.
#[story]
fn variants() -> Element {
    rsx! {
        div { class: "flex flex-wrap items-center gap-3",
            Button { variant: ButtonVariant::Primary, "Primary" }
            Button { variant: ButtonVariant::Secondary, "Secondary" }
            Button { variant: ButtonVariant::Pressed, "Pressed" }
            Button { variant: ButtonVariant::Destructive, "Destructive" }
            Button { variant: ButtonVariant::Failure, "Failure" }
            Button { variant: ButtonVariant::Outline, "Outline" }
            Button { variant: ButtonVariant::Ghost, "Ghost" }
            Button { variant: ButtonVariant::Bare, "Bare" }
        }
    }
}

/// Enabled, disabled, and loading states.
#[story]
fn states() -> Element {
    rsx! {
        div { class: "flex flex-wrap items-center gap-3",
            Button { state: ButtonState::Enabled, "Enabled" }
            Button { state: ButtonState::Disabled, "Disabled" }
            Button { state: ButtonState::Loading, "Saving" }
        }
    }
}

/// Sizes and layout modes.
#[story(name = "Sizes and layout")]
fn sizes() -> Element {
    rsx! {
        div { class: "grid gap-5",
            div { class: "flex flex-wrap items-center gap-3",
                Button { size: ButtonSize::Inline, "Inline" }
                Button { size: ButtonSize::Small, "Small" }
                Button { size: ButtonSize::Medium, "Medium" }
                Button {
                    size: ButtonSize::IconCompact,
                    aria_label: "Compact icon example",
                    "C"
                }
                Button {
                    size: ButtonSize::IconSmall,
                    aria_label: "Small icon example",
                    "S"
                }
                Button {
                    size: ButtonSize::IconMedium,
                    aria_label: "Medium icon example",
                    "M"
                }
            }
            Button {
                layout: ButtonLayout::FullWidthStart,
                variant: ButtonVariant::Outline,
                "Full-width start"
            }
            Button { layout: ButtonLayout::Block, variant: ButtonVariant::Ghost, "Block" }
        }
    }
}

/// Local click state and reset behavior.
#[story]
fn interactive() -> Element {
    let mut clicks = use_signal(|| 0_u32);
    let click_count = clicks();
    let label = if click_count == 1 { "click" } else { "clicks" };

    rsx! {
        div { class: "grid justify-items-start gap-3",
            Button {
                aria_label: "Increment example count",
                onclick: move |_| *clicks.write() += 1,
                "Increment"
            }
            output { class: "text-sm text-ink-2", aria_live: "polite", "{click_count} {label}" }
        }
    }
}

/// Button component.
#[stories(id = "button", name = "Button", thumbnail = thumbnail)]
const BUTTON_STORIES: () = &[variants, states, sizes, interactive];
