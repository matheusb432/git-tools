use dioxus::prelude::*;
use dx_book::{story, variant};
use lucide_dioxus::EllipsisVertical;

use crate::shared::ui::{
    IconPopover, IconPopoverIconMotion, IconPopoverPlacement, MENU_ACTION_HOST_CLASSES,
    MenuActionContent,
};

#[variant(name = "Catalog preview")]
fn preview() -> Element {
    rsx! {
        div { class: "flex min-h-24 items-start justify-end",
            IconPopover {
                id: "story-preview-popover",
                aria_label: "Example actions",
                placement: IconPopoverPlacement::TriggerEnd,
                icon: rsx! {
                    EllipsisVertical { size: 18 }
                },
                div { class: "grid gap-0.5 p-1.5",
                    MenuItem { label: "Edit", description: "Change this item" }
                    MenuItem { label: "Delete", description: "Remove this item" }
                }
            }
        }
    }
}

/// Popover aligned to its trigger.
#[variant(name = "Trigger aligned")]
fn trigger_end() -> Element {
    rsx! {
        div { class: "flex min-h-48 items-start justify-end rounded-panel border border-line bg-surface p-4",
            IconPopover {
                id: "story-trigger-popover",
                aria_label: "Example actions",
                placement: IconPopoverPlacement::TriggerEnd,
                icon_motion: IconPopoverIconMotion::QuarterTurn,
                icon: rsx! {
                    EllipsisVertical { size: 18 }
                },
                div { class: "grid gap-0.5 p-1.5",
                    MenuItem {
                        label: "Open history",
                        description: "Browse saved renders",
                    }
                    MenuItem {
                        label: "Viewer settings",
                        description: "Change local preferences",
                    }
                }
            }
        }
    }
}

/// Popover aligned to the viewport.
#[variant(name = "Viewport aligned")]
fn viewport_end() -> Element {
    rsx! {
        div { class: "flex min-h-48 items-start justify-end rounded-panel border border-line bg-surface p-4",
            IconPopover {
                id: "story-viewport-popover",
                aria_label: "Viewer menu example",
                placement: IconPopoverPlacement::ViewportEnd,
                icon: rsx! {
                    EllipsisVertical { size: 18 }
                },
                div { class: "grid gap-0.5 p-1.5",
                    MenuItem {
                        label: "Recent repositories",
                        description: "Return to prior work",
                    }
                    MenuItem {
                        label: "Keyboard shortcuts",
                        description: "Review available commands",
                    }
                }
            }
        }
    }
}

#[component]
fn MenuItem(label: String, description: String) -> Element {
    rsx! {
        button { class: MENU_ACTION_HOST_CLASSES, r#type: "button",
            MenuActionContent {
                icon: rsx! {
                    span { "#" }
                },
                label,
                description,
            }
        }
    }
}

/// Icon-triggered popover component.
#[story(id = "icon-popover", name = "Icon popover", preview = preview)]
const ICON_POPOVER_STORY: () = &[trigger_end, viewport_end];
