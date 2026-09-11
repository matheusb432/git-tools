use dioxus::prelude::*;

use super::{
    Button, ButtonSize, ButtonVariant,
    popover::{PopoverPlacement, PopoverSurface},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum IconPopoverIconMotion {
    #[default]
    Static,
    #[cfg(feature = "interactive-ui")]
    QuarterTurn,
}

impl IconPopoverIconMotion {
    const fn classes(self) -> &'static str {
        match self {
            Self::Static => "",
            #[cfg(feature = "interactive-ui")]
            Self::QuarterTurn => "icon-popover-quarter-turn",
        }
    }
}

#[component]
pub(crate) fn IconPopover(
    id: String,
    aria_label: String,
    icon: Element,
    #[props(default)] placement: PopoverPlacement,
    #[props(default)] icon_motion: IconPopoverIconMotion,
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
                icon_motion,
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
pub(crate) fn IconPopoverTrigger(
    id: String,
    aria_label: String,
    icon: Element,
    icon_motion: IconPopoverIconMotion,
    trigger_size: ButtonSize,
    trigger_test_id: Option<String>,
    aria_haspopup: Option<String>,
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
            aria_haspopup,
            span { class: icon_motion.classes(), aria_hidden: "true", {icon} }
        }
    }
}
