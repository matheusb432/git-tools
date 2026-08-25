use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const FLOATING_NOTICE_CLASSES: &str = "pointer-events-none fixed inset-x-4 bottom-6 z-50 mx-auto w-fit max-w-3xl wrap-anywhere rounded-panel border bg-surface px-4 py-2 shadow-floating";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FloatingNoticeState {
    #[default]
    Status,
    #[cfg(feature = "desktop")]
    Error,
}

impl FloatingNoticeState {
    const fn classes(self) -> &'static str {
        match self {
            Self::Status => "border-acc-line text-ink",
            #[cfg(feature = "desktop")]
            Self::Error => "border-del-line text-del",
        }
    }
}

#[component]
pub(crate) fn FloatingNotice(
    #[props(default)] state: FloatingNoticeState,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(div {
        class: format!("{FLOATING_NOTICE_CLASSES} {}", state.classes()),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        div { ..attributes,{children} }
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::FloatingNoticeState;

    #[test]
    fn notice_state_owns_its_semantic_colors() {
        assert_eq!(
            FloatingNoticeState::Status.classes(),
            "border-acc-line text-ink"
        );
        assert_eq!(
            FloatingNoticeState::Error.classes(),
            "border-del-line text-del"
        );
    }
}
