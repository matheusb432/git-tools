use dioxus::prelude::*;
use lucide_dioxus::Check;

use super::FieldError;

const CHECKBOX_LABEL_CLASSES: &str =
    "flex min-h-11 w-full items-center gap-3 rounded-sm border px-3 py-2.5 text-left";
const CHECKBOX_MARK_CLASSES: &str = "grid size-5 flex-none place-items-center rounded-sm border-2 peer-focus-visible:ring-2 peer-focus-visible:ring-acc peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-surface";

#[component]
pub(crate) fn Checkbox(
    id: String,
    checked: bool,
    error: Option<String>,
    #[props(default)] disabled: bool,
    onchange: Option<EventHandler<FormEvent>>,
    children: Element,
) -> Element {
    let has_error = error.is_some();
    let error_id = format!("{id}-error");

    rsx! {
        div { class: "w-full",
            label {
                class: "{CHECKBOX_LABEL_CLASSES}",
                class: if checked { "border-acc bg-acc-soft" } else { "border-line-2 bg-surface-2" },
                class: if disabled { "cursor-not-allowed opacity-50" } else { "cursor-pointer" },
                class: if !checked && !disabled { "hover:border-ink-3" } else { "" },
                input {
                    id,
                    class: "peer sr-only",
                    r#type: "checkbox",
                    checked,
                    disabled,
                    aria_invalid: has_error.then_some("true"),
                    aria_describedby: has_error.then_some(error_id.clone()),
                    onchange: move |event| {
                        if let Some(handler) = &onchange {
                            handler.call(event);
                        }
                    },
                }
                span {
                    class: "{CHECKBOX_MARK_CLASSES}",
                    class: if checked { "border-acc bg-acc text-bg" } else { "border-ink-3 bg-transparent text-transparent" },
                    aria_hidden: "true",
                    Check { size: 14 }
                }
                {children}
            }
            FieldError { id: error_id.clone(), message: error }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::Checkbox;

    #[test]
    fn checked_and_disabled_states_layer_over_shared_checkbox_styles() {
        let checked = dioxus_ssr::render_element(rsx! {
            Checkbox { id: "checked", checked: true, "Checked" }
        });
        let disabled = dioxus_ssr::render_element(rsx! {
            Checkbox { id: "disabled", checked: false, disabled: true, "Disabled" }
        });
        let unchecked = dioxus_ssr::render_element(rsx! {
            Checkbox { id: "unchecked", checked: false, "Unchecked" }
        });

        assert!(checked.contains("min-h-11 w-full items-center"));
        assert!(checked.contains("border-acc bg-acc-soft"));
        assert!(checked.contains("cursor-pointer"));
        assert!(disabled.contains("border-line-2 bg-surface-2"));
        assert!(disabled.contains("cursor-not-allowed opacity-50"));
        assert!(!disabled.contains("hover:border-ink-3"));
        assert!(unchecked.contains("hover:border-ink-3"));
    }
}
