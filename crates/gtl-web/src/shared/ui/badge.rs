use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const BADGE_CLASSES: &str = "inline-flex min-h-5 items-center justify-center rounded-sm border px-1.5 font-mono text-[10px] font-medium";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BadgeVariant {
    #[default]
    Neutral,
    Accent,
    Addition,
    Deletion,
}

impl BadgeVariant {
    const fn classes(self) -> &'static str {
        match self {
            Self::Neutral => "border-line-2 bg-sunk text-ink-2",
            Self::Accent => "border-acc-line bg-acc-soft text-acc",
            Self::Addition => "border-add-line bg-add-bg text-add",
            Self::Deletion => "border-del-line bg-del-bg text-del",
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
