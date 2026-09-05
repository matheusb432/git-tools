use dioxus::prelude::*;

use super::{Button, ButtonSize, ButtonVariant};

const VIEWPORT_END_PANEL_CLASSES: &str = "fixed top-[3.25rem] right-3 bottom-auto left-auto m-0 max-h-[calc(100vh-3.75rem)] w-[min(17.25rem,calc(100vw-1rem))] origin-top-right overflow-y-auto rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-floating open:animate-popover-enter motion-reduce:animate-none mobile:top-[3.75rem] mobile:right-2 mobile:max-h-[calc(100vh-4.25rem)]";
const TRIGGER_END_PANEL_CLASSES: &str = "fixed inset-auto m-0 mt-1 max-h-[min(18rem,calc(100vh-1rem))] w-[min(19rem,calc(100vw-1rem))] origin-top-right overflow-y-auto rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-floating [position-area:bottom_span-left] [position-try-fallbacks:flip-block] open:animate-popover-enter motion-reduce:animate-none";
const VIEWPORT_END_WRAPPER_CLASSES: &str = "group/icon-popover my-1 flex-none mobile:my-0.5";
const TRIGGER_END_WRAPPER_CLASSES: &str = "group/icon-popover flex flex-none";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum IconPopoverPlacement {
    #[default]
    ViewportEnd,
    TriggerEnd,
}

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
            Self::QuarterTurn => {
                "transition-transform duration-150 ease-out group-has-[:popover-open]/icon-popover:rotate-90 motion-reduce:transition-none"
            }
        }
    }
}

#[component]
pub(crate) fn IconPopover(
    id: String,
    aria_label: String,
    icon: Element,
    #[props(default)] placement: IconPopoverPlacement,
    #[props(default)] icon_motion: IconPopoverIconMotion,
    #[props(default = ButtonSize::IconSmall)] trigger_size: ButtonSize,
    trigger_test_id: Option<String>,
    wrapper_class: Option<String>,
    children: Element,
) -> Element {
    let wrapper_classes = |base: &str| {
        wrapper_class
            .as_deref()
            .map_or_else(|| base.to_owned(), |extra| format!("{base} {extra}"))
    };
    match placement {
        IconPopoverPlacement::ViewportEnd => rsx! {
            div { class: wrapper_classes(VIEWPORT_END_WRAPPER_CLASSES),
                IconPopoverTrigger {
                    id: id.clone(),
                    aria_label: aria_label.clone(),
                    icon,
                    icon_motion,
                    trigger_size,
                    trigger_test_id,
                }
                div {
                    id,
                    class: VIEWPORT_END_PANEL_CLASSES,
                    popover: "auto",
                    role: "dialog",
                    aria_label,
                    {children}
                }
            }
        },
        IconPopoverPlacement::TriggerEnd => rsx! {
            span { class: wrapper_classes(TRIGGER_END_WRAPPER_CLASSES),
                IconPopoverTrigger {
                    id: id.clone(),
                    aria_label: aria_label.clone(),
                    icon,
                    icon_motion,
                    trigger_size,
                    trigger_test_id,
                }
                span {
                    id,
                    class: TRIGGER_END_PANEL_CLASSES,
                    popover: "auto",
                    role: "dialog",
                    aria_label,
                    {children}
                }
            }
        },
    }
}

#[component]
fn IconPopoverTrigger(
    id: String,
    aria_label: String,
    icon: Element,
    icon_motion: IconPopoverIconMotion,
    trigger_size: ButtonSize,
    trigger_test_id: Option<String>,
) -> Element {
    let trigger_classes = "mobile:size-11 group-has-[:popover-open]/icon-popover:border-acc-line group-has-[:popover-open]/icon-popover:bg-acc-soft group-has-[:popover-open]/icon-popover:text-acc";
    rsx! {
        Button {
            size: trigger_size,
            variant: ButtonVariant::Ghost,
            class: trigger_classes,
            popovertarget: id.clone(),
            popovertargetaction: "toggle",
            aria_label: aria_label.clone(),
            aria_haspopup: "dialog",
            aria_controls: id,
            title: aria_label,
            "data-testid": trigger_test_id,
            span { class: icon_motion.classes(), aria_hidden: "true", {icon} }
        }
    }
}
