use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const SCROLL_AREA_STANDARD_CLASSES: &str = "[scrollbar-color:var(--color-acc-line)_transparent] [&::-webkit-scrollbar]:size-3 [&::-webkit-scrollbar-corner]:bg-transparent [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:border-4 [&::-webkit-scrollbar-thumb]:border-solid [&::-webkit-scrollbar-thumb]:border-transparent [&::-webkit-scrollbar-thumb]:bg-acc-line [&::-webkit-scrollbar-thumb]:bg-clip-content [&::-webkit-scrollbar-thumb:hover]:bg-acc [&::-webkit-scrollbar-thumb:active]:bg-acc-2";
const SCROLL_AREA_RAIL_CLASSES: &str = "[scrollbar-color:var(--color-acc)_transparent] [scrollbar-width:thin] [&::-webkit-scrollbar]:size-1.5 [&::-webkit-scrollbar-corner]:bg-transparent [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-acc [&::-webkit-scrollbar-thumb:hover]:bg-acc-2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ScrollAreaVariant {
    #[default]
    Standard,
    Rail,
}

impl ScrollAreaVariant {
    const fn classes(self) -> &'static str {
        match self {
            Self::Standard => SCROLL_AREA_STANDARD_CLASSES,
            Self::Rail => SCROLL_AREA_RAIL_CLASSES,
        }
    }
}

#[component]
pub(crate) fn ScrollArea(
    #[props(default)] variant: ScrollAreaVariant,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: variant.classes(),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { ..attributes,{children} }
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
