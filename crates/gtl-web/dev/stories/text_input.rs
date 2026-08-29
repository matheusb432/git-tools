use dioxus::prelude::*;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::TextInput;

pub(super) const VARIANTS: &[StoryVariant] = &[
    StoryVariant {
        slug: "default",
        title: "Default",
        summary: "Label, placeholder, and helper text.",
        render: default_input,
    },
    StoryVariant {
        slug: "validation",
        title: "Validation",
        summary: "Invalid state with a text error.",
        render: validation,
    },
    StoryVariant {
        slug: "disabled",
        title: "Disabled",
        summary: "Labeled disabled state.",
        render: disabled,
    },
    StoryVariant {
        slug: "controlled",
        title: "Controlled",
        summary: "Local input state.",
        render: controlled,
    },
];

pub(super) fn preview(_context: StoryContext) -> Element {
    rsx! {
        div { class: "w-full max-w-md",
            TextInput {
                label: "Label",
                placeholder: "Enter text...",
                supporting_content: rsx! {
                    span { "Helper text." }
                },
            }
        }
    }
}

fn default_input(_context: StoryContext) -> Element {
    rsx! {
        div { class: "max-w-md",
            TextInput {
                label: "Branch filter",
                placeholder: "feature/component-preview",
                supporting_content: rsx! {
                    span { "Match by branch name or commit ID." }
                },
            }
        }
    }
}

fn validation(_context: StoryContext) -> Element {
    rsx! {
        div { class: "max-w-md",
            TextInput {
                label: "Remote name",
                value: "origin main",
                aria_invalid: "true",
                aria_describedby: "remote-name-error",
                supporting_content: rsx! {
                    span { id: "remote-name-error", class: "text-del", "Remote names cannot contain spaces." }
                },
            }
        }
    }
}

fn disabled(_context: StoryContext) -> Element {
    rsx! {
        div { class: "max-w-md",
            TextInput {
                label: "Repository root",
                value: "/workspace/example",
                disabled: true,
                supporting_content: rsx! {
                    span { "Owned by the active viewer session." }
                },
            }
        }
    }
}

fn controlled(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        ControlledInput { key: "{generation}" }
    }
}

#[component]
fn ControlledInput() -> Element {
    let mut value = use_signal(String::new);
    let visible_value = value();

    rsx! {
        div { class: "grid max-w-md gap-3",
            TextInput {
                label: "Commit message",
                value: visible_value.clone(),
                placeholder: "Describe the change",
                oninput: move |event: FormEvent| value.set(event.value()),
            }
            output { class: "min-h-5 text-sm text-ink-2", aria_live: "polite",
                if visible_value.is_empty() {
                    "Waiting for input"
                } else {
                    "Preview: {visible_value}"
                }
            }
        }
    }
}
