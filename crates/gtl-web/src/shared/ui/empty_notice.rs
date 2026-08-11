use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const EMPTY_NOTICE_CLASSES: &str =
    "rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic";

#[component]
pub(crate) fn EmptyNotice(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(p {
        class: EMPTY_NOTICE_CLASSES,
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        p { ..attributes,{children} }
    }
}
