use dioxus::prelude::*;
use dx_story::{stories, story};

use crate::shared::ui::{FieldLabelVisibility, TextArea};

#[story(name = "Catalog thumbnail")]
fn thumbnail() -> Element {
    rsx! {
        div { class: "max-w-sm",
            TextArea {
                id: "story-text-area-thumbnail",
                label: "Comment",
                placeholder: "Leave a comment",
                rows: "3",
            }
        }
    }
}

/// Label, placeholder, and helper text.
#[story]
fn default() -> Element {
    rsx! {
        div { class: "max-w-md",
            TextArea {
                id: "story-release-notes",
                label: "Release notes",
                placeholder: "Summarize the change",
                rows: "4",
                supporting_content: rsx! {
                    span { "Plain text; line breaks are kept." }
                },
            }
        }
    }
}

/// Invalid state: the error sits below the textarea and describes it before the helper text.
#[story]
fn validation() -> Element {
    rsx! {
        div { class: "max-w-md",
            TextArea {
                id: "story-commit-body",
                label: "Commit body",
                value: "Fix the lexer.\nAlso other things.",
                rows: "4",
                error: "Describe each change on its own line.",
                supporting_content: rsx! {
                    span { "Shown below the subject in history." }
                },
            }
        }
    }
}

/// Visually hidden label for a control whose context is already on screen.
#[story]
fn hidden_label() -> Element {
    rsx! {
        div { class: "grid max-w-md gap-2",
            p { class: "text-xs text-ink-3", "src/lexer.rs, lines 40–41" }
            TextArea {
                id: "story-line-note",
                label: "Comment on src/lexer.rs lines 40–41",
                label_visibility: FieldLabelVisibility::Hidden,
                placeholder: "Leave a comment",
                rows: "3",
            }
        }
    }
}

/// Read-only text kept in place while a request runs.
#[story]
fn read_only() -> Element {
    rsx! {
        div { class: "max-w-md",
            TextArea {
                id: "story-saving-note",
                label: "Comment",
                value: "Keep the end-of-input token and report its offset.",
                rows: "3",
                readonly: true,
                aria_busy: "true",
            }
        }
    }
}

/// Local input state; Ctrl+Enter submits.
#[story]
fn controlled() -> Element {
    let mut value = use_signal(String::new);
    let mut submitted = use_signal(String::new);
    let visible_submission = submitted();

    rsx! {
        div { class: "grid max-w-md gap-3",
            TextArea {
                id: "story-draft-note",
                label: "Comment",
                value: value(),
                placeholder: "Type, then press Ctrl+Enter",
                rows: "3",
                oninput: move |event: FormEvent| value.set(event.value()),
                onkeydown: move |event: KeyboardEvent| {
                    if event.key() == Key::Enter && event.modifiers().ctrl() {
                        event.prevent_default();
                        submitted.set(value());
                    }
                },
            }
            output { class: "min-h-5 text-sm text-ink-2", aria_live: "polite",
                if visible_submission.is_empty() {
                    "Nothing submitted"
                } else {
                    "Submitted: {visible_submission}"
                }
            }
        }
    }
}

/// Text area component.
#[stories(id = "text-area", name = "Text area", thumbnail = thumbnail)]
const TEXT_AREA_STORIES: () = &[default, validation, hidden_label, read_only, controlled];
