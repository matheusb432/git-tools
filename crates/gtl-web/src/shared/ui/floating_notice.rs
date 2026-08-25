use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const FLOATING_NOTICE_CLASSES: &str = "pointer-events-none fixed inset-x-4 bottom-6 z-50 mx-auto w-fit max-w-3xl wrap-anywhere rounded-panel border border-acc-line bg-surface px-4 py-2 text-ink shadow-floating";

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
