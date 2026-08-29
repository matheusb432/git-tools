use dioxus::prelude::*;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::{Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant};

pub(super) const VARIANTS: &[StoryVariant] = &[
    StoryVariant {
        slug: "variants",
        title: "Variants",
        summary: "All visual variants.",
        render: variants,
    },
    StoryVariant {
        slug: "states",
        title: "States",
        summary: "Enabled, disabled, and loading.",
        render: states,
    },
    StoryVariant {
        slug: "sizes",
        title: "Sizes and layout",
        summary: "Sizes and layout modes.",
        render: sizes,
    },
    StoryVariant {
        slug: "interactive",
        title: "Interactive",
        summary: "Local click state and reset.",
        render: interactive,
    },
];

pub(super) fn preview(_context: StoryContext) -> Element {
    rsx! {
        div { class: "grid grid-cols-2 gap-3",
            Button { variant: ButtonVariant::Primary, "Primary" }
            Button { variant: ButtonVariant::Secondary, "Secondary" }
            Button { variant: ButtonVariant::Ghost, "Ghost" }
            Button { state: ButtonState::Disabled, "Disabled" }
        }
    }
}

fn variants(_context: StoryContext) -> Element {
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

fn states(_context: StoryContext) -> Element {
    rsx! {
        div { class: "flex flex-wrap items-center gap-3",
            Button { state: ButtonState::Enabled, "Enabled" }
            Button { state: ButtonState::Disabled, "Disabled" }
            Button { state: ButtonState::Loading, "Saving" }
        }
    }
}

fn sizes(_context: StoryContext) -> Element {
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

fn interactive(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        InteractiveButton { key: "{generation}" }
    }
}

#[component]
fn InteractiveButton() -> Element {
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
