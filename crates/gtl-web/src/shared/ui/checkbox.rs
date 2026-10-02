use dioxus::prelude::*;

use super::FieldError;

#[component]
pub(crate) fn Checkbox(
    id: String,
    label: String,
    hint: Option<String>,
    error: Option<String>,
    checked: bool,
    #[props(default)] disabled: bool,
    onchange: EventHandler<bool>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let hint_id = format!("{id}-hint");
    let error_id = format!("{id}-error");
    let described_by = if hint.is_some() {
        format!("{hint_id} {error_id}")
    } else {
        error_id.clone()
    };
    let invalid = error.is_some();
    rsx! {
        div { class: "control-checkbox-field", ..attributes,
            label { class: "control-choice", r#for: id.clone(),
                input {
                    id,
                    r#type: "checkbox",
                    class: "control-checkbox",
                    checked,
                    disabled,
                    aria_describedby: described_by,
                    aria_invalid: invalid.then_some("true"),
                    onchange: move |event: FormEvent| onchange.call(event.checked()),
                }
                span {
                    span { class: "control-choice-label", "{label}" }
                    if let Some(hint) = hint {
                        span { id: hint_id, class: "control-choice-hint", "{hint}" }
                    }
                }
            }
            FieldError { id: error_id, message: error }
        }
    }
}
