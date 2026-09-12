use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use lucide_dioxus::ChevronDown;

use super::FieldError;

const SELECT_CLASSES: &str = "control-select min-h-10 w-full px-3 py-2 pr-10 peer";

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum SelectVariant {
    #[default]
    Field,
    #[cfg(feature = "desktop")]
    Toolbar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectOption {
    value: String,
    label: String,
    disabled: bool,
}

impl SelectOption {
    pub(crate) fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }
}

#[component]
pub(crate) fn Select(
    id: String,
    aria_label: String,
    value: String,
    options: Vec<SelectOption>,
    error: Option<String>,
    #[props(default)] variant: SelectVariant,
    #[props(default)] disabled: bool,
    #[props(extends = GlobalAttributes)]
    #[props(extends = select)]
    attributes: Vec<Attribute>,
    onchange: Option<EventHandler<FormEvent>>,
) -> Element {
    let has_error = error.is_some();
    let error_id = format!("{id}-error");
    let selected_value = value.clone();
    let base = attributes!(select {
        class: SELECT_CLASSES,
        id: id.clone(),
        value,
        disabled,
        aria_label,
        aria_invalid: has_error.then_some("true"),
        aria_describedby: has_error.then_some(error_id.clone()),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { class: "w-full",
            div { class: "relative w-full",
                select {
                    onchange: move |event| {
                        if let Some(handler) = &onchange {
                            handler.call(event);
                        }
                    },
                    ..attributes,
                    for option in options {
                        option {
                            key: "{option.value}",
                            value: option.value.clone(),
                            selected: option.value == selected_value,
                            disabled: option.disabled,
                            "{option.label}"
                        }
                    }
                }
                span {
                    class: "control-select-error-overlay",
                    aria_hidden: "true",
                }
                span { class: "control-select-chevron", aria_hidden: "true",
                    ChevronDown { size: 17 }
                }
            }
            if variant == SelectVariant::Field || has_error {
                FieldError { id: error_id, message: error }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::{Select, SelectOption};

    #[test]
    fn invalid_select_connects_the_control_to_a_stable_error_slot() {
        let html = dioxus_ssr::render_element(rsx! {
            Select {
                id: "layout",
                aria_label: "Layout",
                value: "",
                options: vec![SelectOption::new("", "Choose a layout")],
                error: "Choose a diff layout.",
            }
        });

        assert!(html.contains("aria-invalid=\"true\""));
        assert!(html.contains("aria-describedby=\"layout-error\""));
        assert!(html.contains("id=\"layout-error\""));
        assert!(html.contains("role=\"alert\""));
        assert!(html.contains("control-field-error"));
    }

    #[test]
    fn selected_value_marks_the_matching_option() {
        let html = dioxus_ssr::render_element(rsx! {
            Select {
                id: "layout",
                aria_label: "Layout",
                value: "split",
                options: vec![
                    SelectOption::new("unified", "Unified"),
                    SelectOption::new("split", "Side by side"),
                ],
                error: None,
            }
        });

        let selected = html
            .split("<option")
            .find(|option| option.contains(">Side by side</option>"))
            .unwrap_or_default();
        assert!(selected.contains("selected"));
    }
}
