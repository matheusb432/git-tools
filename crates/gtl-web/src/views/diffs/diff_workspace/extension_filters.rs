use dioxus::prelude::*;
use gtl_models::{
    diffs::{ExtensionFilter, ExtensionFilterMode},
    settings::ViewerLanguage,
};
use lucide_dioxus::{ListFilter, RotateCcw};

use crate::shared::{
    i18n::{t, use_language},
    ui::{
        Button, ButtonLayout, ButtonSize, ButtonVariant, ExtensionSelectionAction,
        ExtensionSelectionInput, IconPopover, popover::PopoverPlacement,
    },
};

pub(super) mod desktop;

/// Edits the extension filter behind the Files panel's filter button.
#[component]
pub(crate) fn ExtensionFilterMenu(
    id: String,
    filter: ExtensionFilter,
    available: Vec<String>,
    hidden_count: usize,
    onchange: EventHandler<ExtensionFilter>,
    children: Element,
) -> Element {
    let language = use_language();
    let label = filter_label(&filter, language);
    let mode = filter.mode();
    let extensions = filter.extensions().clone();
    let description = match mode {
        ExtensionFilterMode::Only => t!(language, "extensions-mode-only-description"),
        ExtensionFilterMode::Hide => t!(language, "extensions-mode-hide-description"),
    };
    rsx! {
        IconPopover {
            id: id.clone(),
            aria_label: label,
            placement: PopoverPlacement::TriggerStart,
            icon: rsx! {
                span { class: "extension-filter-trigger",
                    ListFilter { size: 16 }
                    if filter.is_active() {
                        span { class: "extension-filter-active-dot" }
                    }
                }
            },
            div { class: "extension-filter",
                div { class: "extension-filter-header",
                    h2 { class: "text-sm font-semibold", {t!(language, "extensions-filter-label")} }
                    ExtensionFilterModeToggle {
                        mode,
                        onchange: {
                            let extensions = extensions.clone();
                            move |mode| onchange.call(ExtensionFilter::new(mode, extensions.clone()))
                        },
                    }
                }
                p { class: "text-xs text-ink-3", "{description}" }
                ExtensionSelectionInput {
                    id: format!("{id}-extensions"),
                    label: t!(language, "extensions-selected"),
                    selected: extensions.clone(),
                    available,
                    onchange: move |action: ExtensionSelectionAction| {
                        onchange.call(ExtensionFilter::new(mode, action.apply(&extensions)));
                    },
                }
                {children}
                div { class: "extension-filter-footer",
                    if hidden_count > 0 {
                        p { class: "text-xs",
                            {t!(language, "extensions-hidden-count", count = hidden_count)}
                        }
                    }
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        layout: ButtonLayout::FullWidthStart,
                        disabled: !filter.is_active(),
                        onclick: move |_| onchange.call(ExtensionFilter::default()),
                        RotateCcw { size: 15 }
                        {t!(language, "extensions-clear")}
                    }
                }
            }
        }
    }
}

#[component]
fn ExtensionFilterModeToggle(
    mode: ExtensionFilterMode,
    onchange: EventHandler<ExtensionFilterMode>,
) -> Element {
    let language = use_language();
    rsx! {
        div {
            class: "extension-filter-mode",
            role: "group",
            aria_label: t!(language, "extensions-mode-label"),
            for (value, label) in [
                (ExtensionFilterMode::Only, t!(language, "extensions-mode-only")),
                (ExtensionFilterMode::Hide, t!(language, "extensions-mode-hide")),
            ]
            {
                Button {
                    key: "{value}",
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Toggle,
                    aria_pressed: (mode == value).to_string(),
                    onclick: move |_| {
                        if mode != value {
                            onchange.call(value);
                        }
                    },
                    "{label}"
                }
            }
        }
    }
}

fn filter_label(filter: &ExtensionFilter, language: ViewerLanguage) -> String {
    if !filter.is_active() {
        return t!(language, "extensions-filter-label");
    }
    let extensions = filter
        .extensions()
        .extensions()
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(", ");
    match filter.mode() {
        ExtensionFilterMode::Only => t!(
            language,
            "extensions-filter-label-only",
            extensions = extensions.as_str()
        ),
        ExtensionFilterMode::Hide => t!(
            language,
            "extensions-filter-label-hide",
            extensions = extensions.as_str()
        ),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        diffs::{ExtensionFilter, ExtensionFilterMode, FileExtensions},
        settings::ViewerLanguage,
    };

    use super::filter_label;

    #[test]
    fn trigger_label_names_the_active_rule() {
        let only = ExtensionFilter::new(
            ExtensionFilterMode::Only,
            FileExtensions::new(["rs", "toml"]),
        );
        let inactive = ExtensionFilter::new(ExtensionFilterMode::Only, FileExtensions::default());

        assert_eq!(
            filter_label(&only, ViewerLanguage::EnUs),
            "Filter by extension: showing only .rs, .toml"
        );
        assert_eq!(
            filter_label(&inactive, ViewerLanguage::PtBr),
            "Filtrar por extensão"
        );
    }
}
