use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const COUNT_BADGE_CLASSES: &str = "control-count-badge min-w-5 px-1 text-xs";

#[component]
pub(crate) fn CountBadge(
    count: usize,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let base = attributes!(span {
        class: COUNT_BADGE_CLASSES,
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        span { ..attributes,"{count}" }
    }
}
