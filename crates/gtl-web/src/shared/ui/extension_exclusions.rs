mod search;

use dioxus::prelude::*;
use gtl_models::diffs::ExcludedExtensions;
use lucide_dioxus::{Plus, X};

use crate::shared::{
    file_extension::FileExtension,
    i18n::{t, use_language},
    ui::{Button, ButtonLayout, ButtonSize, ButtonVariant},
};

#[derive(Clone)]
pub(crate) enum ExtensionExclusionsAction {
    Toggle(String),
    Exclude(FileExtension),
    Reveal(String),
}

impl ExtensionExclusionsAction {
    pub(crate) fn apply(self, excluded: &ExcludedExtensions) -> ExcludedExtensions {
        let mut values = excluded.extensions().to_vec();
        match self {
            Self::Toggle(value) if !values.contains(&value) => values.push(value),
            Self::Toggle(value) | Self::Reveal(value) => values.retain(|entry| entry != &value),
            Self::Exclude(value) => values.push(value.as_str().to_owned()),
        }
        ExcludedExtensions::new(values)
    }
}

#[component]
pub(crate) fn ExtensionExclusionsInput(
    id: String,
    excluded: ExcludedExtensions,
    available: Vec<String>,
    #[props(default)] disabled: bool,
    onchange: EventHandler<ExtensionExclusionsAction>,
) -> Element {
    let language = use_language();
    let mut searching = use_signal(|| false);
    let add_id = format!("{id}-add");
    let search_id = format!("{id}-search");
    let results_id = format!("{id}-results");
    rsx! {
        div { class: "extension-exclusions-input",
            div {
                class: "extension-filter-chips",
                aria_label: t!(language, "extensions-excluded"),
                if excluded.is_empty() {
                    p { class: "text-sm text-ink-3", {t!(language, "extensions-none")} }
                }
                for extension in excluded.extensions().iter().cloned() {
                    ExcludedChip {
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
                    excluded: excluded.clone(),
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
                    {t!(language, "extensions-exclude")}
                }
            }
        }
    }
}

#[component]
fn ExcludedChip(
    extension: String,
    disabled: bool,
    onchange: EventHandler<ExtensionExclusionsAction>,
) -> Element {
    let language = use_language();
    let value = extension.clone();
    rsx! {
        Button {
            class: "extension-filter-chip",
            size: ButtonSize::Small,
            variant: ButtonVariant::Bare,
            aria_label: t!(language, "extensions-reveal", extension = extension.as_str()),
            disabled,
            onclick: move |_| {
                onchange.call(ExtensionExclusionsAction::Reveal(value.clone()));
            },
            code { class: "truncate", ".{extension}" }
            X { size: 13 }
        }
    }
}
