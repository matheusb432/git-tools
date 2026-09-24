use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

use super::FieldError;

const TEXT_INPUT_CLASSES: &str = "control-text-input h-9 w-full px-2.5 font-mono";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TextInputLabelVisibility {
    #[default]
    Visible,
    Hidden,
}

/// Renders a labeled single-line text field.
///
/// `id` names the input and derives the ids that `aria-describedby` links: the validation
/// message from `error` appears below the input, followed by `supporting_content`.
#[component]
pub(crate) fn TextInput(
    id: String,
    label: String,
    #[props(default)] label_visibility: TextInputLabelVisibility,
    error: Option<String>,
    supporting_content: Option<Element>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = input)]
    attributes: Vec<Attribute>,
    oninput: Option<EventHandler<FormEvent>>,
) -> Element {
    let error_id = format!("{id}-error");
    let description_id = format!("{id}-description");
    let described_by = [
        error.is_some().then_some(error_id.as_str()),
        supporting_content
            .is_some()
            .then_some(description_id.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    let base = attributes!(input {
        class: TEXT_INPUT_CLASSES,
        id,
        aria_invalid: error.is_some().then_some("true"),
        aria_describedby: (!described_by.is_empty()).then_some(described_by),
    });
    let attributes = merge_attributes(vec![attributes, base]);
    let hidden = label_visibility == TextInputLabelVisibility::Hidden;

    rsx! {
        div { class: if hidden { "min-w-0" } else { "grid min-w-0 gap-1.5" },
            label { class: if hidden { "block min-w-0" } else { "grid min-w-0 gap-1.5" },
                span { class: if hidden { "sr-only" } else { "font-semibold text-ink" }, "{label}" }
                input {
                    oninput: move |event| {
                        if let Some(handler) = &oninput {
                            handler.call(event);
                        }
                    },
                    ..attributes,
                }
            }
            if error.is_some() {
                FieldError { id: error_id, message: error }
            }
            if let Some(supporting_content) = supporting_content {
                div { id: description_id, class: "leading-5 text-ink-2", {supporting_content} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::TextInput;

    #[test]
    fn invalid_input_describes_itself_with_its_error_and_supporting_text() {
        let html = dioxus_ssr::render_element(rsx! {
            TextInput {
                id: "branch",
                label: "Comparison branch",
                error: "Enter a local branch name.",
                supporting_content: rsx! { "Used when the current branch has no upstream." },
            }
        });

        assert!(html.contains("id=\"branch\""));
        assert!(html.contains("aria-invalid=\"true\""));
        assert!(html.contains("aria-describedby=\"branch-error branch-description\""));
        assert!(html.contains("id=\"branch-error\""));
        assert!(html.contains("role=\"alert\""));
        assert!(html.contains("id=\"branch-description\""));
    }

    #[test]
    fn valid_input_without_supporting_text_describes_nothing() {
        let html = dioxus_ssr::render_element(rsx! {
            TextInput { id: "search", label: "Search" }
        });

        assert!(!html.contains("aria-invalid"));
        assert!(!html.contains("aria-describedby"));
        assert!(!html.contains("control-field-error"));
    }
}
