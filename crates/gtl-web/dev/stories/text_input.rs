use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::TextInput;

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "max-w-sm",
            TextInput {
                id: "story-search",
                label: "Search",
                placeholder: "Type to filter",
                supporting_content: rsx! {
                    span { "Matches names and identifiers." }
                },
            }
        }
    }
}

/// Label, placeholder, and helper text.
#[story]
fn default() -> Element {
    rsx! {
        div { class: "max-w-md",
            TextInput {
                id: "story-branch-filter",
                label: "Branch filter",
                placeholder: "feature/component-preview",
                supporting_content: rsx! {
                    span { "Match by branch name or commit ID." }
                },
            }
        }
    }
}

/// Invalid state: the error sits below the input and describes it before the helper text.
#[story]
fn validation() -> Element {
    rsx! {
        div { class: "max-w-md",
            TextInput {
                id: "story-remote-name",
                label: "Remote name",
                value: "origin main",
                error: "Remote names cannot contain spaces.",
                supporting_content: rsx! {
                    span { "Used as the push destination." }
                },
            }
        }
    }
}

/// Labeled disabled state.
#[story]
fn disabled() -> Element {
    rsx! {
        div { class: "max-w-md",
            TextInput {
                id: "story-repository-root",
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

/// Local input state.
#[story]
fn controlled() -> Element {
    let mut value = use_signal(String::new);
    let visible_value = value();

    rsx! {
        div { class: "grid max-w-md gap-3",
            TextInput {
                id: "story-commit-message",
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

/// Text input component.
#[stories(id = "text-input", name = "Text input", thumbnail = thumbnail)]
const TEXT_INPUT_STORIES: () = &[default, validation, disabled, controlled];
