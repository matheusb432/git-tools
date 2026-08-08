use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SeparatorOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl SeparatorOrientation {
    const fn classes(self) -> &'static str {
        match self {
            Self::Horizontal => "h-px w-full",
            Self::Vertical => "h-full w-px",
        }
    }
}

#[component]
pub(crate) fn Separator(
    #[props(default)] orientation: SeparatorOrientation,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let base = attributes!(div {
        class: format!("shrink-0 bg-line {}", orientation.classes()),
        role: "none",
        aria_hidden: "true",
        "data-orientation": match orientation {
            SeparatorOrientation::Horizontal => "horizontal",
            SeparatorOrientation::Vertical => "vertical",
        },
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { ..attributes }
    }
}
