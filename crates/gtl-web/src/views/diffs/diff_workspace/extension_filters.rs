use dioxus::prelude::*;
use gtl_models::diffs::ExcludedExtensions;
use lucide_dioxus::{ChevronDown, FileX, RotateCcw};

use crate::shared::{
    i18n::{t, use_language},
    ui::{
        Button, ButtonLayout, ButtonSize, ButtonVariant, ExtensionExclusionsAction,
        ExtensionExclusionsInput, IconPopover, popover::PopoverPlacement,
    },
};

#[cfg(feature = "desktop")]
pub(super) mod desktop;

#[component]
pub(crate) fn ExtensionFilter(
    excluded: ExcludedExtensions,
    available: Vec<String>,
    onchange: EventHandler<ExtensionExclusionsAction>,
    onrestore: EventHandler<()>,
    children: Element,
) -> Element {
    let language = use_language();
    let summary = excluded
        .extensions()
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(", ");
    let label = if summary.is_empty() {
        t!(language, "extensions-label-none")
    } else {
        t!(language, "extensions-label", extensions = summary.as_str())
    };
    let summary_text = if summary.is_empty() {
        t!(language, "extensions-summary-none")
    } else {
        summary.clone()
    };
    rsx! {
        IconPopover {
            id: "diff-extension-filters",
            aria_label: label,
            placement: PopoverPlacement::TriggerEnd,
            trigger_size: ButtonSize::Small,
            icon: rsx! {
                span { class: "extension-filter-trigger",
                    FileX { size: 16 }
                    code { class: "min-w-0 flex-1 truncate text-left mobile:hidden", "{summary_text}" }
                    ChevronDown { size: 12 }
                }
            },
            div { class: "extension-filter",
                h2 { class: "text-sm font-semibold", {t!(language, "extensions-excluded")} }
                ExtensionExclusionsInput {
                    id: "extension-filter",
                    excluded,
                    available,
                    onchange,
                }
                {children}
                div { class: "extension-filter-footer",
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        layout: ButtonLayout::FullWidthStart,
                        onclick: move |_| onrestore.call(()),
                        RotateCcw { size: 15 }
                        {t!(language, "extensions-restore-defaults")}
                    }
                }
            }
        }
    }
}
