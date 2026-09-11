use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const LOADING_SPINNER_CLASSES: &str = "control-loading-spinner size-3.5";

#[component]
pub(crate) fn LoadingSpinner(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let base = attributes!(span {
        class: LOADING_SPINNER_CLASSES,
        aria_hidden: "true",
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        span {..attributes,
            svg {
                class: "control-loading-spinner-icon size-full",
                view_box: "0 0 24 24",
                fill: "none",
                "focusable": "false",
                circle {
                    cx: "12",
                    cy: "12",
                    r: "9",
                    stroke: "currentColor",
                    stroke_width: "2",
                    opacity: "0.22",
                }
                path {
                    d: "M12 3a9 9 0 1 1-9 9",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                }
            }
        }
    }
}
