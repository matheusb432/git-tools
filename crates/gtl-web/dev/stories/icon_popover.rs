use dioxus::prelude::*;
use lucide_dioxus::EllipsisVertical;

use super::{StoryContext, StoryVariant};
use crate::shared::ui::{
    IconPopover, IconPopoverIconMotion, IconPopoverPlacement, MENU_ACTION_HOST_CLASSES,
    MenuActionContent,
};

pub(super) const VARIANTS: &[StoryVariant] = &[
    StoryVariant {
        slug: "trigger-end",
        title: "Trigger aligned",
        summary: "Popover aligned to its trigger.",
        render: trigger_end,
    },
    StoryVariant {
        slug: "viewport-end",
        title: "Viewport aligned",
        summary: "Popover aligned to the viewport.",
        render: viewport_end,
    },
];

pub(super) fn preview(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        div { class: "flex min-h-24 items-center justify-center",
            IconPopover {
                key: "{generation}",
                id: "story-preview-popover",
                aria_label: "Example actions",
                placement: IconPopoverPlacement::TriggerEnd,
                icon_motion: IconPopoverIconMotion::QuarterTurn,
                icon: rsx! {
                    EllipsisVertical { size: 18 }
                },
                div { class: "grid gap-0.5 p-1.5",
                    MenuItem { label: "Edit item", description: "Change this item" }
                    MenuItem { label: "Delete", description: "Remove this item" }
                }
            }
        }
    }
}

fn trigger_end(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        div { class: "flex min-h-48 items-start justify-end rounded-panel border border-line bg-surface p-4",
            IconPopover {
                key: "{generation}",
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

fn viewport_end(context: StoryContext) -> Element {
    let generation = context.reset_generation;
    rsx! {
        div { class: "flex min-h-48 items-start justify-end rounded-panel border border-line bg-surface p-4",
            IconPopover {
                key: "{generation}",
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
