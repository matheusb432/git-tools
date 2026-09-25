mod search;

use dioxus::prelude::*;
use gtl_models::diffs::FileExtensions;
use lucide_dioxus::{Plus, X};

use crate::shared::{
    file_extension::FileExtension,
    i18n::{t, use_language},
    ui::{Button, ButtonLayout, ButtonSize, ButtonVariant},
};

#[derive(Clone)]
pub(crate) enum ExtensionSelectionAction {
    Toggle(String),
    Add(FileExtension),
    Remove(String),
}

impl ExtensionSelectionAction {
    pub(crate) fn apply(self, selected: &FileExtensions) -> FileExtensions {
        let mut values = selected.extensions().to_vec();
        match self {
            Self::Toggle(value) if !values.contains(&value) => values.push(value),
            Self::Toggle(value) | Self::Remove(value) => values.retain(|entry| entry != &value),
            Self::Add(value) => values.push(value.as_str().to_owned()),
        }
        FileExtensions::new(values)
    }
}

/// Edits a set of file extensions through removable chips and a searchable add list.
#[component]
pub(crate) fn ExtensionSelectionInput(
    id: String,
    label: String,
    selected: FileExtensions,
    available: Vec<String>,
    #[props(default)] disabled: bool,
    onchange: EventHandler<ExtensionSelectionAction>,
) -> Element {
    let language = use_language();
    let mut searching = use_signal(|| false);
    let add_id = format!("{id}-add");
    let search_id = format!("{id}-search");
    let results_id = format!("{id}-results");
    rsx! {
        div { class: "extension-selection-input",
            div { class: "extension-filter-chips", aria_label: label,
                if selected.is_empty() {
                    p { class: "text-sm text-ink-3", {t!(language, "extensions-none")} }
                }
                for extension in selected.extensions().iter().cloned() {
                    SelectedChip {
                        key: "{extension}",
                        extension,
                        disabled,
                        onchange,
                    }
                }
            }
            if searching() {
                search::ExtensionSearch {
                    input_id: search_id,
                    results_id,
                    add_id: add_id.clone(),
                    available,
                    selected: selected.clone(),
                    disabled,
                    onchange,
                    onclose: move |()| searching.set(false),
                }
            } else {
                Button {
                    id: add_id,
                    class: "extension-filter-add",
                    size: ButtonSize::Medium,
                    layout: ButtonLayout::FullWidthStart,
                    variant: ButtonVariant::Outline,
                    disabled,
                    onclick: move |_| searching.set(true),
                    Plus { size: 16 }
                    {t!(language, "extensions-add-extension")}
                }
            }
        }
    }
}

#[component]
fn SelectedChip(
    extension: String,
    disabled: bool,
    onchange: EventHandler<ExtensionSelectionAction>,
) -> Element {
    let language = use_language();
    let value = extension.clone();
    rsx! {
        Button {
            class: "extension-filter-chip",
            size: ButtonSize::Small,
            variant: ButtonVariant::Bare,
            aria_label: t!(language, "extensions-remove", extension = extension.as_str()),
            disabled,
            onclick: move |_| {
                onchange.call(ExtensionSelectionAction::Remove(value.clone()));
            },
            code { class: "truncate", ".{extension}" }
            X { size: 13 }
        }
    }
}
