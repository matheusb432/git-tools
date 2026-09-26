use dioxus::prelude::*;

use super::{
    Button, ButtonSize, ButtonVariant,
    popover::{PopoverPlacement, PopoverSurface},
};

#[component]
pub(crate) fn IconPopover(
    id: String,
    aria_label: String,
    icon: Element,
    #[props(default)] placement: PopoverPlacement,
    #[props(default = ButtonSize::IconSmall)] trigger_size: ButtonSize,
    trigger_test_id: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        span {
            class: "icon-popover group/icon-popover",
            "data-placement": placement.as_str(),
            IconPopoverTrigger {
                id: id.clone(),
                aria_label: aria_label.clone(),
                icon,
                trigger_size,
                trigger_test_id,
            }
            PopoverSurface {
                id,
                placement,
                role: "group",
                aria_label: aria_label.clone(),
                {children}
            }
        }
    }
}

#[component]
fn IconPopoverTrigger(
    id: String,
    aria_label: String,
    icon: Element,
    trigger_size: ButtonSize,
    trigger_test_id: Option<String>,
) -> Element {
    rsx! {
        Button {
            id: format!("{id}-trigger"),
            size: trigger_size,
            variant: ButtonVariant::Ghost,
            class: "icon-popover-trigger mobile:size-11",
            popovertarget: id.clone(),
            popovertargetaction: "toggle",
            aria_label: aria_label.clone(),
            aria_controls: id,
            title: aria_label,
            "data-testid": trigger_test_id,
            span { class: "inline-flex", aria_hidden: "true", {icon} }
        }
    }
}
