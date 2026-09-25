use dioxus::prelude::*;
use gtl_wire::viewer::ViewerTheme;
use lucide_dioxus::CircleDot;

use crate::shared::{
    i18n::{t, use_language},
    viewer_theme::{VIEWER_THEME_OPTIONS, viewer_theme_from_value, viewer_theme_label},
};

#[component]
pub(crate) fn ViewerThemePicker(
    theme: ViewerTheme,
    disabled: bool,
    onthemechange: EventHandler<ViewerTheme>,
) -> Element {
    let language = use_language();
    rsx! {
        label { class: "control-viewer-theme-picker h-8 gap-2 px-2.5",
            span { class: "text-acc", aria_hidden: "true",
                CircleDot { size: 9, fill: "currentColor" }
            }
            span { class: "sr-only", {t!(language, "settings-theme")} }
            select {
                class: "control-viewer-theme-select",
                value: theme.as_str(),
                disabled,
                aria_label: t!(language, "settings-theme"),
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
