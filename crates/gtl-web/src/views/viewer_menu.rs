use dioxus::prelude::*;
use lucide_dioxus::{Ellipsis, Settings};

use crate::shared::{
    browser,
    ui::{
        ButtonSize, IconPopoverIconMotion, MENU_ACTION_HOST_CLASSES, MenuActionContent,
        icon_popover::IconPopoverTrigger,
        menu_keyboard,
        popover::{PopoverPlacement, PopoverSurface},
    },
};

#[component]
pub(crate) fn ViewerMenu(
    id: String,
    #[props(default = ButtonSize::IconSmall)] trigger_size: ButtonSize,
    trigger_test_id: Option<String>,
    onsettings: EventHandler<()>,
) -> Element {
    let keyboard_id = id.clone();
    rsx! {
        span {
            class: "icon-popover group/icon-popover",
            onkeydown: move |event| menu_keyboard::keydown(
                &keyboard_id,
                &format!("{keyboard_id}-trigger"),
                &event,
            ),
            IconPopoverTrigger {
                id: id.clone(),
                aria_label: "Viewer menu",
                aria_haspopup: "menu",
                trigger_size,
                trigger_test_id,
                icon_motion: IconPopoverIconMotion::QuarterTurn,
                icon: rsx! {
                    Ellipsis { size: 18 }
                },
            }
            PopoverSurface {
                id: id.clone(),
                placement: PopoverPlacement::TriggerEnd,
                role: "menu",
                aria_label: "Viewer menu",
                aria_labelledby: format!("{id}-trigger"),
                div { class: "grid gap-0.5 p-1.5",
                    button {
                        class: MENU_ACTION_HOST_CLASSES,
                        r#type: "button",
                        role: "menuitem",
                        tabindex: "-1",
                        autofocus: true,
                        aria_label: "User settings",
                        onclick: move |_| {
                            browser::hide_popover(&id);
                            onsettings.call(());
                        },
                        MenuActionContent {
                            icon: rsx! {
                                Settings { size: 15 }
                            },
                            label: "Settings",
                            description: "Viewer defaults",
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::ViewerMenu;

    #[component]
    fn ViewerMenuTestView() -> Element {
        rsx! {
            ViewerMenu { id: "viewer-menu-test", onsettings: move |()| {} }
        }
    }

    #[test]
    fn contains_only_settings_action() {
        let html = dioxus_ssr::render_element(rsx! {
            ViewerMenuTestView {}
        });

        assert!(!html.contains("History"));
        assert!(html.contains("Settings"));
        assert!(!html.contains("Theme"));
        assert!(!html.contains("<select"));
    }
}
