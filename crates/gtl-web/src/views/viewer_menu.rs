mod keyboard;

use dioxus::prelude::*;
use lucide_dioxus::{Ellipsis, History, Settings};

use crate::shared::{
    browser,
    ui::{
        ButtonSize, CountBadge, IconPopoverIconMotion, MENU_ACTION_HOST_CLASSES, MenuActionContent,
        icon_popover::IconPopoverTrigger,
        popover::{PopoverPlacement, PopoverSurface},
    },
};

#[component]
pub(crate) fn ViewerMenu(
    id: String,
    history_count: usize,
    #[props(default = ButtonSize::IconSmall)] trigger_size: ButtonSize,
    trigger_test_id: Option<String>,
    history_test_id: Option<String>,
    onhistory: EventHandler<()>,
    onsettings: EventHandler<()>,
) -> Element {
    let keyboard_id = id.clone();
    rsx! {
        span {
            class: "icon-popover group/icon-popover",
            onkeydown: move |event| keyboard::keydown(&keyboard_id, &event),
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
                        aria_label: "History",
                        "data-testid": history_test_id,
                        onclick: {
                            let popover_id = id.clone();
                            move |_| {
                                browser::hide_popover(&popover_id);
                                onhistory.call(());
                            }
                        },
                        MenuActionContent {
                            icon: rsx! {
                                History { size: 15 }
                            },
                            label: "History",
                            description: "Browse saved renders",
                            if history_count > 0 {
                                CountBadge { count: history_count }
                            }
                        }
                    }
                    button {
                        class: MENU_ACTION_HOST_CLASSES,
                        r#type: "button",
                        role: "menuitem",
                        tabindex: "-1",
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
            ViewerMenu {
                id: "viewer-menu-test",
                history_count: 2,
                onhistory: move |()| {},
                onsettings: move |()| {},
            }
        }
    }

    #[test]
    fn contains_only_history_and_settings_actions() {
        let html = dioxus_ssr::render_element(rsx! {
            ViewerMenuTestView {}
        });

        assert!(html.contains("History"));
        assert!(html.contains("Settings"));
        assert!(!html.contains("Theme"));
        assert!(!html.contains("<select"));
    }
}
