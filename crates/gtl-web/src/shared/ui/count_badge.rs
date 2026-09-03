use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const COUNT_BADGE_CLASSES: &str = "inline-flex shrink-0 items-center justify-center rounded-full bg-acc-soft text-center text-acc";

#[component]
pub(crate) fn CountBadge(
    count: usize,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let base = attributes!(span {
        class: format!("{COUNT_BADGE_CLASSES} min-w-5 px-1 text-xs"),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        span { ..attributes,"{count}" }
    }
}
