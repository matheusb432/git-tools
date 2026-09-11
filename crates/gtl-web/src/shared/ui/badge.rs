use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const BADGE_CLASSES: &str = "control-badge min-h-5 px-1.5 font-mono text-xs font-medium";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BadgeVariant {
    #[default]
    Neutral,
    // TODO: uncomment and implement on commits panel
    // Selected,
    Addition,
    Deletion,
}

impl BadgeVariant {
    const fn classes(self) -> &'static str {
        match self {
            Self::Neutral => "control-badge-neutral",
            // Self::Selected => "border-acc bg-acc text-bg",
            Self::Addition => "control-badge-addition",
            Self::Deletion => "control-badge-deletion",
        }
    }
}

#[component]
pub(crate) fn Badge(
    #[props(default)] variant: BadgeVariant,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(span {
        class: format!("{BADGE_CLASSES} {}", variant.classes()),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        span { ..attributes,{children} }
    }
}
