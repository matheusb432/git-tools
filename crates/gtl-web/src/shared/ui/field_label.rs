use dioxus::prelude::*;

#[component]
pub(crate) fn FieldLabel(for_id: String, label: String, hint: Option<String>) -> Element {
    rsx! {
        label {
            r#for: for_id,
            class: "control-field-label min-w-0 gap-0.5 text-sm font-semibold",
            "{label}"
            if let Some(hint) = hint {
                span { class: "text-xs font-normal text-ink-3", "{hint}" }
            }
        }
    }
}
