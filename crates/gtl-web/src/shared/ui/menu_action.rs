use dioxus::prelude::*;

pub(crate) const MENU_ACTION_HOST_CLASSES: &str =
    "control-menu-action min-h-12 w-full gap-2 px-2 py-1.5 group/action";

#[component]
pub(crate) fn MenuActionContent(
    icon: Element,
    label: String,
    description: Option<String>,
    children: Option<Element>,
) -> Element {
    rsx! {
        span { class: "control-menu-action-icon size-7", aria_hidden: "true", {icon} }
        span { class: "min-w-0 flex-1",
            strong { class: "block text-xs font-semibold text-inherit", "{label}" }
            if let Some(description) = description {
                small { class: "mt-0.5 block truncate text-xs text-ink-3", "{description}" }
            }
        }
        {children}
    }
}
