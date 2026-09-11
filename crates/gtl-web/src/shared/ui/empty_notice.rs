use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const EMPTY_NOTICE_CLASSES: &str = "control-empty-notice p-4";

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
