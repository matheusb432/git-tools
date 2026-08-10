use dioxus::prelude::*;

use crate::shared::ui::code_text::CodeText;

#[component]
pub(super) fn DiffExtensionExclusions(file_extensions: Vec<String>) -> Element {
    rsx! {
        if file_extensions.is_empty() {
            span { class: "text-ink-3", "None" }
        } else {
            for extension in file_extensions {
                CodeText { "*.{extension}" }
            }
        }
    }
}
