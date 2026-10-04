use dioxus::prelude::*;

use super::FieldError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FieldLabelVisibility {
    #[default]
    Visible,
    Hidden,
}

pub(super) struct TextFieldDescription {
    pub(super) error_id: String,
    pub(super) supporting_content_id: String,
    pub(super) described_by: Option<String>,
}

impl TextFieldDescription {
    pub(super) fn new(id: &str, has_error: bool, has_supporting_content: bool) -> Self {
        let error_id = format!("{id}-error");
        let supporting_content_id = format!("{id}-description");
        let described_by = [
            has_error.then_some(error_id.as_str()),
            has_supporting_content.then_some(supporting_content_id.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        Self {
            error_id,
            supporting_content_id,
            described_by: (!described_by.is_empty()).then_some(described_by),
        }
    }
}

#[component]
pub(super) fn TextFieldFrame(
    label: String,
    label_visibility: FieldLabelVisibility,
    error_id: String,
    error: Option<String>,
    supporting_content_id: String,
    supporting_content: Option<Element>,
    children: Element,
) -> Element {
    let hidden = label_visibility == FieldLabelVisibility::Hidden;

    rsx! {
        div { class: if hidden { "min-w-0" } else { "grid min-w-0 gap-1.5" },
            label { class: if hidden { "block min-w-0" } else { "grid min-w-0 gap-1.5" },
                span { class: if hidden { "sr-only" } else { "font-semibold text-ink" }, "{label}" }
                {children}
            }
            if error.is_some() {
                FieldError { id: error_id, message: error }
            }
            if let Some(supporting_content) = supporting_content {
                div { id: supporting_content_id, class: "leading-5 text-ink-2",
                    {supporting_content}
                }
            }
        }
    }
}
