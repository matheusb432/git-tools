use dioxus::prelude::*;

const NAVIGATION_BAR_CLASSES: &str = "control-navigation-bar h-9";
const NAVIGATION_BAR_LEADING_CLASSES: &str = "control-navigation-bar-leading h-9";
const NAVIGATION_BAR_RAIL_CLASSES: &str = "control-navigation-bar-rail h-9 min-w-0";
const NAVIGATION_BAR_TRAILING_CLASSES: &str = "control-navigation-bar-trailing h-9";

#[component]
pub(crate) fn NavigationBar(
    aria_label: String,
    #[props(default = true)] bordered: bool,
    leading: Option<Element>,
    rail: Element,
    trailing: Option<Element>,
) -> Element {
    rsx! {
        nav {
            aria_label,
            class: NAVIGATION_BAR_CLASSES,
            "data-bordered": bordered.to_string(),
            if let Some(leading) = leading {
                div { class: NAVIGATION_BAR_LEADING_CLASSES, {leading} }
            }
            div { class: NAVIGATION_BAR_RAIL_CLASSES, {rail} }
            if let Some(trailing) = trailing {
                div { class: NAVIGATION_BAR_TRAILING_CLASSES, {trailing} }
            }
        }
    }
}
