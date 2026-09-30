use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};
use lucide_dioxus::{Check, ChevronDown};
use wasm_bindgen::JsCast as _;

use super::{FieldError, menu_keyboard};
use crate::shared::browser;

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum SelectVariant {
    #[default]
    Field,
    Toolbar,
}

impl SelectVariant {
    const fn trigger_classes(self) -> &'static str {
        match self {
            Self::Field => "control-select min-h-10 w-full gap-2 px-3 py-2",
            Self::Toolbar => "control-select min-h-8 w-full gap-2 px-2.5 py-1",
        }
    }
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

/// Chooses one option from a themed listbox anchored below its trigger.
///
/// The trigger keeps `id`; the listbox is `{id}-listbox`, and each option carries its value in
/// `data-value`. `onchange` receives only values that differ from `value`.
#[component]
pub(crate) fn Select(
    id: String,
    aria_label: String,
    value: String,
    options: Vec<SelectOption>,
    error: Option<String>,
    #[props(default)] variant: SelectVariant,
    #[props(default)] disabled: bool,
    icon: Option<Element>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    onchange: Option<EventHandler<String>>,
) -> Element {
    let mut expanded = use_signal(|| false);
    let has_error = error.is_some();
    let error_id = format!("{id}-error");
    let listbox_id = format!("{id}-listbox");
    let anchor_name = format!("--{id}");
    let selected_label = options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.clone())
        .unwrap_or_default();
    let base = attributes!(button {
        class: variant.trigger_classes(),
        id: id.clone(),
        r#type: "button",
        role: "combobox",
        value: value.clone(),
        disabled,
        popovertarget: listbox_id.clone(),
        aria_label: aria_label.clone(),
        aria_haspopup: "listbox",
        aria_controls: listbox_id.clone(),
        aria_expanded: expanded().to_string(),
        aria_invalid: has_error.then_some("true"),
        aria_describedby: has_error.then_some(error_id.clone()),
        style: format!("anchor-name: {anchor_name};"),
    });
    let attributes = merge_attributes(vec![attributes, base]);
    let keyboard_listbox_id = listbox_id.clone();
    let keyboard_trigger_id = id.clone();
    let toggle_listbox_id = listbox_id.clone();

    rsx! {
        div {
            class: "w-full",
            onkeydown: move |event| {
                menu_keyboard::keydown(&keyboard_listbox_id, &keyboard_trigger_id, &event);
            },
            button {..attributes,
                if let Some(icon) = icon {
                    span { class: "control-select-icon", aria_hidden: "true", {icon} }
                }
                span { class: "min-w-0 flex-1 truncate", "{selected_label}" }
                span { class: "control-select-chevron", aria_hidden: "true",
                    ChevronDown { size: 16 }
                }
            }
            div {
                id: listbox_id.clone(),
                class: "control-select-listbox",
                style: "position-anchor: {anchor_name};",
                popover: "auto",
                role: "listbox",
                aria_label,
                ontoggle: move |_| {
                    let open = browser::popover_is_open(&toggle_listbox_id);
                    expanded.set(open);
                    if open {
                        focus_selected_option(&toggle_listbox_id);
                    }
                },
                for option in options {
                    SelectListboxOption {
                        key: "{option.value}",
                        selected: option.value == value,
                        option,
                        listbox_id: listbox_id.clone(),
                        trigger_id: id.clone(),
                        onchange,
                    }
                }
            }
            if variant == SelectVariant::Field || has_error {
                FieldError { id: error_id, message: error }
            }
        }
    }
}

fn focus_selected_option(listbox_id: &str) {
    let Some(listbox) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(listbox_id))
    else {
        return;
    };
    let option = listbox
        .query_selector("[role='option'][aria-selected='true']:enabled")
        .ok()
        .flatten()
        .or_else(|| {
            listbox
                .query_selector("[role='option']:enabled")
                .ok()
                .flatten()
        })
        .and_then(|option| option.dyn_into::<web_sys::HtmlElement>().ok());
    if let Some(option) = option {
        let _ = option.focus();
    }
}

#[component]
fn SelectListboxOption(
    option: SelectOption,
    selected: bool,
    listbox_id: String,
    trigger_id: String,
    onchange: Option<EventHandler<String>>,
) -> Element {
    let SelectOption {
        value,
        label,
        disabled,
    } = option;
    rsx! {
        button {
            class: "control-select-option",
            r#type: "button",
            role: "option",
            tabindex: "-1",
            aria_selected: selected.to_string(),
            disabled,
            "data-value": value.clone(),
            onclick: move |_| {
                browser::hide_popover(&listbox_id);
                browser::focus_element(trigger_id.clone());
                if !selected && let Some(handler) = &onchange {
                    handler.call(value.clone());
                }
            },
            span { class: "min-w-0 flex-1 truncate", "{label}" }
            span { class: "control-select-check", aria_hidden: "true",
                Check { size: 14 }
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
    fn selected_value_labels_the_trigger_and_marks_its_option() {
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

        let trigger = html.split("role=\"listbox\"").next().unwrap_or_default();
        assert!(trigger.contains("role=\"combobox\""));
        assert!(trigger.contains("aria-controls=\"layout-listbox\""));
        assert!(trigger.contains(">Side by side<"));
        let selected = html
            .split("role=\"option\"")
            .skip(1)
            .find(|option| option.contains(">Side by side<"))
            .unwrap_or_default();
        assert!(selected.contains("aria-selected=\"true\""));
        assert!(selected.contains("data-value=\"split\""));
    }
}
