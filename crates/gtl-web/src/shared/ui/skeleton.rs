use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[component]
pub(crate) fn Skeleton(#[props(extends = GlobalAttributes)] attributes: Vec<Attribute>) -> Element {
    let base = attributes!(div {
        class: "animate-pulse rounded-sm bg-line-2 motion-reduce:animate-none",
        aria_hidden: "true",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { ..attributes }
    }
}
