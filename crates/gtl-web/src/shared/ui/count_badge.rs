use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const COUNT_BADGE_CLASSES: &str = "inline-flex shrink-0 items-center justify-center rounded-full bg-acc-soft text-center text-acc";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CountBadgeSize {
    #[cfg(feature = "artifact")]
    Compact,
    #[default]
    Regular,
}

impl CountBadgeSize {
    const fn classes(self) -> &'static str {
        match self {
            #[cfg(feature = "artifact")]
            Self::Compact => "min-w-4 px-1 py-0.5 text-[9px] font-semibold leading-none",
            Self::Regular => "min-w-5 px-1 text-xs",
        }
    }
}

#[component]
pub(crate) fn CountBadge(
    count: usize,
    #[props(default)] size: CountBadgeSize,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let base = attributes!(span {
        class: format!("{COUNT_BADGE_CLASSES} {}", size.classes()),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        span { ..attributes,"{count}" }
    }
}
