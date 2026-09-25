use dioxus::prelude::*;

use super::{ScrollAreaVariant, browser::ScrollbarController};
use crate::shared::i18n::{t, use_language};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollbarPlacement {
    Viewport(ScrollAreaVariant),
    FileBottom,
}

impl ScrollbarPlacement {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Viewport(_) => "viewport",
            Self::FileBottom => "file-bottom",
        }
    }

    const fn axes(self) -> &'static [ScrollAxis] {
        match self {
            Self::Viewport(ScrollAreaVariant::Standard) => {
                &[ScrollAxis::Horizontal, ScrollAxis::Vertical]
            }
            Self::Viewport(ScrollAreaVariant::Vertical) => &[ScrollAxis::Vertical],
            Self::Viewport(ScrollAreaVariant::Rail) | Self::FileBottom => &[ScrollAxis::Horizontal],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScrollAxis {
    Horizontal,
    Vertical,
}

impl ScrollAxis {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}

#[component]
pub(crate) fn ScrollbarRails(
    controller: ScrollbarController,
    placement: ScrollbarPlacement,
    target_id: Option<String>,
) -> Element {
    rsx! {
        span {
            class: "scrollbars",
            "data-scrollbars": placement.as_str(),
            "data-scroll-target": target_id,
            onmounted: move |event| controller.mount(&event),
            onpointerdown: move |event| controller.pointer_down(&event),
            onpointermove: move |event| controller.pointer_move(&event),
            onpointerup: move |_| controller.pointer_end(),
            onpointercancel: move |_| controller.pointer_end(),
            onlostpointercapture: move |_| controller.pointer_end(),
            onkeydown: move |event| controller.keydown(&event),
            for axis in placement.axes().iter().copied() {
                ScrollbarRail { key: "{axis.as_str()}", axis, placement }
            }
        }
    }
}

#[component]
fn ScrollbarRail(axis: ScrollAxis, placement: ScrollbarPlacement) -> Element {
    let language = use_language();
    let label = match (placement, axis) {
        (ScrollbarPlacement::FileBottom, _) => t!(language, "scrollbar-diff-horizontal"),
        (_, ScrollAxis::Horizontal) => t!(language, "scrollbar-horizontal"),
        (_, ScrollAxis::Vertical) => t!(language, "scrollbar-vertical"),
    };
    rsx! {
        span {
            class: "scrollbar-rail",
            role: "scrollbar",
            aria_label: label,
            aria_orientation: axis.as_str(),
            aria_valuemin: "0",
            aria_valuemax: "0",
            aria_valuenow: "0",
            aria_hidden: "true",
            tabindex: "-1",
            "data-scroll-axis": axis.as_str(),
            "data-gtl-horizontal-rail": (placement == ScrollbarPlacement::FileBottom).then_some(""),
            span { class: "scrollbar-thumb" }
        }
    }
}
