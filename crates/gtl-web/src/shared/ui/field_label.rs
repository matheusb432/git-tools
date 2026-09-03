use dioxus::prelude::*;

#[component]
pub(crate) fn FieldLabel(for_id: String, label: String, hint: Option<String>) -> Element {
    rsx! {
        label {
            r#for: for_id,
            class: "flex min-w-0 flex-1 flex-col gap-0.5 text-sm font-semibold text-ink",
            "{label}"
            if let Some(hint) = hint {
                span { class: "text-xs font-normal text-ink-3", "{hint}" }
            }
        }
    }
}
