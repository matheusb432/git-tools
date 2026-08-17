use dioxus::prelude::*;
use gtl_models::diffs::ExcludedExtensions;

use crate::shared::ui::code_text::CodeText;

#[component]
pub(super) fn DiffExtensionExclusions(file_extensions: ExcludedExtensions) -> Element {
    rsx! {
        if file_extensions.is_empty() {
            span { class: "text-ink-3", "None" }
        } else {
            for extension in file_extensions.extensions() {
                CodeText { "*.{extension}" }
            }
        }
    }
}
