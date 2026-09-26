use dioxus::prelude::*;
use lucide_dioxus::Settings;

use crate::shared::{
    i18n::{t, use_language},
    ui::{Button, ButtonSize, ButtonVariant},
};

#[component]
pub(crate) fn ViewerSettingsButton(onsettings: EventHandler<()>) -> Element {
    let label = t!(use_language(), "viewer-settings");
    rsx! {
        Button {
            id: "viewer-settings-button",
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: label.clone(),
            title: label,
            onclick: move |_| onsettings.call(()),
            span { class: "inline-flex", aria_hidden: "true",
                Settings { size: 16 }
            }
        }
    }
}
