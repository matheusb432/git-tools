pub(crate) mod browser;
mod geometry;
pub(crate) mod scrollbar;

use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const SCROLL_AREA_STANDARD_CLASSES: &str = "scroll-area";
const SCROLL_AREA_RAIL_CLASSES: &str = "scroll-area-rail";
const SCROLL_AREA_VERTICAL_CLASSES: &str = "scroll-area-vertical";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ScrollAreaVariant {
    #[default]
    Standard,
    Rail,
    Vertical,
}

impl ScrollAreaVariant {
    const fn classes(self) -> &'static str {
        match self {
            Self::Standard => SCROLL_AREA_STANDARD_CLASSES,
            Self::Rail => SCROLL_AREA_RAIL_CLASSES,
            Self::Vertical => SCROLL_AREA_VERTICAL_CLASSES,
        }
    }
}

#[component]
pub(crate) fn ScrollArea(
    #[props(default)] variant: ScrollAreaVariant,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    onmounted: Option<EventHandler<MountedEvent>>,
    onresize: Option<EventHandler<ResizeEvent>>,
    onscroll: Option<EventHandler<ScrollEvent>>,
    onfocusin: Option<EventHandler<FocusEvent>>,
    onfocusout: Option<EventHandler<FocusEvent>>,
    children: Element,
) -> Element {
    let scrollbars = browser::use_scrollbars();
    let base = attributes!(div {
        class: variant.classes(),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div {
            onmounted: move |event| {
                if let Some(onmounted) = onmounted {
                    onmounted.call(event);
                }
            },
            onresize: move |event| {
                scrollbars.measure();
                if let Some(onresize) = onresize {
                    onresize.call(event);
                }
            },
            onscroll: move |event| {
                scrollbars.refresh();
                if let Some(onscroll) = onscroll {
                    onscroll.call(event);
                }
            },
            onfocusin: move |event| {
                if let Some(handler) = onfocusin {
                    handler.call(event);
                }
            },
            onfocusout: move |event| {
                if let Some(handler) = onfocusout {
                    handler.call(event);
                }
            },
            ..attributes,
            scrollbar::ScrollbarRails {
                controller: scrollbars,
                placement: scrollbar::ScrollbarPlacement::Viewport(variant),
            }
            div {
                class: "scroll-area-content",
                onresize: move |_| scrollbars.measure(),
                {children}
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::{SCROLL_AREA_RAIL_CLASSES, SCROLL_AREA_STANDARD_CLASSES, ScrollAreaVariant};

    #[test]
    fn scroll_area_variant_owns_its_scrollbar_treatment() {
        assert_eq!(
            ScrollAreaVariant::Standard.classes(),
            SCROLL_AREA_STANDARD_CLASSES
        );
        assert_eq!(ScrollAreaVariant::Rail.classes(), SCROLL_AREA_RAIL_CLASSES);
    }
}
