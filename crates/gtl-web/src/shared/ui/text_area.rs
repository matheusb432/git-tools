use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

use super::text_field::{FieldLabelVisibility, TextFieldDescription, TextFieldFrame};

const TEXT_AREA_CLASSES: &str = "control-text-input control-text-area w-full px-2.5 font-mono";

/// `id` names the textarea and derives the ids that `aria-describedby` links: the validation
/// message from `error` appears below the textarea, followed by `supporting_content`.
#[component]
pub(crate) fn TextArea(
    id: String,
    label: String,
    #[props(default)] label_visibility: FieldLabelVisibility,
    error: Option<String>,
    supporting_content: Option<Element>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = textarea)]
    attributes: Vec<Attribute>,
    oninput: Option<EventHandler<FormEvent>>,
    onkeydown: Option<EventHandler<KeyboardEvent>>,
) -> Element {
    let description = TextFieldDescription::new(&id, error.is_some(), supporting_content.is_some());
    let base = attributes!(textarea {
        class: TEXT_AREA_CLASSES,
        id,
        aria_invalid: error.is_some().then_some("true"),
        aria_describedby: description.described_by,
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        TextFieldFrame {
            label,
            label_visibility,
            error_id: description.error_id,
            error,
            supporting_content_id: description.supporting_content_id,
            supporting_content,
            textarea {
                onkeydown: move |event| {
                    if let Some(handler) = onkeydown {
                        handler.call(event);
                    }
                },
                oninput: move |event| {
                    if let Some(handler) = &oninput {
                        handler.call(event);
                    }
                },
                ..attributes,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::{FieldLabelVisibility, TextArea};
    use crate::test_support::TestResult;

    #[test]
    fn invalid_text_area_describes_itself_with_its_error_and_supporting_text() {
        let html = dioxus_ssr::render_element(rsx! {
            TextArea {
                id: "review-note",
                label: "Comment",
                error: "The comment was not saved.",
                supporting_content: rsx! { "Ctrl+Enter saves." },
            }
        });

        assert!(html.contains("<textarea"));
        assert!(html.contains("id=\"review-note\""));
        assert!(html.contains("aria-invalid=\"true\""));
        assert!(html.contains("aria-describedby=\"review-note-error review-note-description\""));
        assert!(html.contains("id=\"review-note-error\""));
        assert!(html.contains("id=\"review-note-description\""));
    }

    #[test]
    fn hidden_label_still_names_the_text_area() -> TestResult {
        let html = dioxus_ssr::render_element(rsx! {
            TextArea {
                id: "review-note",
                label: "Comment on line 42",
                label_visibility: FieldLabelVisibility::Hidden,
            }
        });

        let label = html
            .split_once("<label")
            .and_then(|(_, label)| label.split_once("</label>"))
            .map(|(label, _)| label)
            .ok_or("the text area has no wrapping label")?;
        assert!(label.contains("Comment on line 42"));
        assert!(label.contains("<textarea"));
        assert!(label.contains("id=\"review-note\""));
        assert!(!html.contains("aria-describedby"));
        Ok(())
    }
}
