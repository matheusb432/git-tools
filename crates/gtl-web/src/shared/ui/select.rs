use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use lucide_dioxus::ChevronDown;

use super::FieldError;

const SELECT_CLASSES: &str = "peer block min-h-10 w-full cursor-pointer appearance-none rounded-sm border border-line-2 bg-surface-2 px-3 py-2 pr-10 text-ink outline-none hover:border-ink-3 focus-visible:border-acc focus-visible:ring-2 focus-visible:ring-acc-soft disabled:cursor-not-allowed disabled:opacity-50";

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
    #[props(default)] disabled: bool,
    #[props(extends = GlobalAttributes)]
    #[props(extends = select)]
    attributes: Vec<Attribute>,
    onchange: Option<EventHandler<FormEvent>>,
) -> Element {
    let has_error = error.is_some();
    let error_id = format!("{id}-error");
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
                            value: option.value,
                            disabled: option.disabled,
                            "{option.label}"
                        }
                    }
                }
                span {
                    class: if has_error { "pointer-events-none absolute inset-0 rounded-sm border border-del opacity-100 duration-150 motion-safe:transition-opacity motion-reduce:transition-none peer-focus-visible:ring-2 peer-focus-visible:ring-del-bg" } else { "pointer-events-none absolute inset-0 rounded-sm border border-del opacity-0 duration-150 motion-safe:transition-opacity motion-reduce:transition-none peer-focus-visible:ring-2 peer-focus-visible:ring-del-bg" },
                    aria_hidden: "true",
                }
                span {
                    class: "pointer-events-none absolute top-1/2 right-3 grid -translate-y-1/2 place-items-center text-ink-3 peer-focus:text-acc peer-disabled:opacity-50",
                    aria_hidden: "true",
                    ChevronDown { size: 17 }
                }
            }
            FieldError { id: error_id, message: error }
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
        assert!(html.contains("h-4"));
    }
}
