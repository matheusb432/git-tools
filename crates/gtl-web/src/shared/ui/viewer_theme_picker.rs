use dioxus::prelude::*;
use gtl_wire::viewer::ViewerTheme;
use lucide_dioxus::CircleDot;

use crate::shared::viewer_theme::{
    VIEWER_THEME_OPTIONS, viewer_theme_from_value, viewer_theme_label,
};

#[component]
pub(crate) fn ViewerThemePicker(
    theme: ViewerTheme,
    disabled: bool,
    onthemechange: EventHandler<ViewerTheme>,
) -> Element {
    rsx! {
        label { class: "flex h-8 flex-none items-center gap-2 rounded-sm border border-transparent bg-transparent px-2.5 text-ink-2 hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-within:border-acc-line has-[select:disabled]:cursor-not-allowed has-[select:disabled]:opacity-50",
            span { class: "text-acc", aria_hidden: "true",
                CircleDot { size: 9, fill: "currentColor" }
            }
            span { class: "sr-only", "Theme" }
            select {
                class: "cursor-pointer appearance-none bg-transparent text-inherit outline-none disabled:cursor-not-allowed",
                value: theme.as_str(),
                disabled,
                aria_label: "Theme",
                onchange: move |event| {
                    if let Some(theme) = viewer_theme_from_value(&event.value()) {
                        onthemechange.call(theme);
                    }
                },
                for option in VIEWER_THEME_OPTIONS {
                    option { value: option.as_str(), "{viewer_theme_label(option)}" }
                }
            }
        }
    }
}
