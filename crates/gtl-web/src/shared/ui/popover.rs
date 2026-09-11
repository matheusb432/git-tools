use dioxus::prelude::*;

use super::scroll_area::ScrollArea;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PopoverPlacement {
    #[default]
    ViewportEnd,
    TriggerEnd,
}

impl PopoverPlacement {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ViewportEnd => "viewport-end",
            Self::TriggerEnd => "trigger-end",
        }
    }
}

#[component]
pub(crate) fn PopoverSurface(
    id: String,
    #[props(default)] placement: PopoverPlacement,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    rsx! {
        span {
            id,
            class: "popover-surface",
            popover: "auto",
            "data-placement": placement.as_str(),
            ..attributes,
            ScrollArea { class: "max-h-[inherit]", {children} }
        }
    }
}
