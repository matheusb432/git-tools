use dioxus::prelude::*;
use gtl_contracts::viewer::{ViewerActiveView, ViewerAppliedExclusions};

use crate::shared::ui::{Button, ButtonSize, ButtonVariant};

#[component]
pub(super) fn ViewTitlebar(
    view: ViewerActiveView,
    files_folded: bool,
    copy_context_enabled: bool,
    mobile_navigation: Option<Element>,
    onfold: EventHandler<bool>,
    oncontext: EventHandler<bool>,
) -> Element {
    rsx! {
        header { class: "col-span-3 row-start-1 flex min-w-0 items-center gap-4 border-b border-line bg-surface px-5 py-3 tablet:flex-wrap tablet:gap-2.5 tablet:px-3 tablet:py-2.5 mobile:gap-1.5 mobile:px-2 mobile:py-2",
            div { class: "flex min-w-0 items-baseline gap-2 text-lg font-semibold tracking-tight mobile:text-base",
                span { class: "truncate",
                    "~/"
                    b { class: "font-bold text-acc", "{view.repository_name}" }
                }
                span { class: "flex-none self-center rounded-sm border border-acc-line bg-acc-soft px-2 py-0.5 text-xs font-medium text-acc",
                    "{view.title}"
                }
            }
            div { class: "flex min-w-0 items-center gap-1.5 text-ink-2 tablet:order-3 tablet:w-full",
                span { class: "truncate text-acc", "{view.branch}" }
                if !view.upstream.is_empty() {
                    span { class: "text-ink-3", "→" }
                    span { class: "truncate text-ink-3", "{view.upstream}" }
                }
            }
            if let Some(exclusions) = &view.exclusions {
                span {
                    class: "flex-none cursor-help whitespace-nowrap rounded-sm border border-del-line bg-del-bg px-2 py-0.5 text-xs font-semibold text-del",
                    title: exclusion_tooltip(exclusions),
                    {exclusion_label(exclusions)}
                }
            }
            div { class: "flex-1" }
            if let Some(mobile_navigation) = mobile_navigation {
                {mobile_navigation}
            }
            div { class: "flex items-center gap-2 mobile:hidden",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    title: "Collapse or expand all files",
                    onclick: move |_| onfold.call(!files_folded),
                    if files_folded {
                        "Expand all"
                    } else {
                        "Collapse all"
                    }
                }
                Button {
                    size: ButtonSize::Small,
                    variant: if copy_context_enabled { ButtonVariant::Pressed } else { ButtonVariant::Outline },
                    aria_pressed: copy_context_enabled.to_string(),
                    title: "Prepend a commented path and line range when copying code",
                    onclick: move |_| oncontext.call(!copy_context_enabled),
                    "+ context"
                }
            }
        }
    }
}

fn exclusion_label(exclusions: &ViewerAppliedExclusions) -> String {
    let hidden_count = exclusions.hidden_paths.len();
    let extension_label = if exclusions.extensions.is_empty() {
        "configured".to_owned()
    } else {
        exclusions.extensions.join(", ")
    };
    format!(
        "{hidden_count} file{} hidden · {extension_label}",
        super::plural_suffix(hidden_count)
    )
}

fn exclusion_tooltip(exclusions: &ViewerAppliedExclusions) -> String {
    let mut tooltip = String::from("Hidden by git-tools config [diff.exclude]:");
    for path in &exclusions.hidden_paths {
        tooltip.push('\n');
        tooltip.push_str(path);
    }
    tooltip
}
