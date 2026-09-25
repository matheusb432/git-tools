use dioxus::prelude::*;

use crate::shared::i18n::{t, use_language};

#[component]
pub(crate) fn NoData() -> Element {
    let language = use_language();
    rsx! {
        span {
            class: "font-mono text-xs font-normal text-ink-3",
            role: "img",
            aria_label: t!(language, "no-data"),
            "-"
        }
    }
}
