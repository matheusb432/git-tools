use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const FLOATING_NOTICE_CLASSES: &str = "control-floating-notice mx-auto w-fit px-4 py-2";

#[component]
pub(crate) fn FloatingNotice(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: FLOATING_NOTICE_CLASSES,
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { ..attributes,{children} }
    }
}
