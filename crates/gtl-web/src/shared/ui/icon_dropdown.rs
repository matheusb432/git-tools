use dioxus::prelude::*;
use lucide_dioxus::Ellipsis;

use super::{Button, ButtonSize, ButtonVariant};

const PANEL_CLASSES: &str = "fixed top-[3.25rem] right-3 bottom-auto left-auto z-70 m-0 max-h-[calc(100vh-3.75rem)] w-[min(17.25rem,calc(100vw-1rem))] origin-top-right overflow-y-auto rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-floating open:animate-popover-enter motion-reduce:animate-none mobile:top-[3.75rem] mobile:right-2 mobile:max-h-[calc(100vh-4.25rem)]";

#[component]
pub(crate) fn IconDropdown(
    id: String,
    aria_label: String,
    trigger_test_id: String,
    children: Element,
) -> Element {
    let trigger_target = id.clone();
    let trigger_label = aria_label.clone();

    rsx! {
        div { class: "group/icon-dropdown mb-2 flex-none mobile:mb-1",
            Button {
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                class: "mobile:size-11 group-has-[:popover-open]/icon-dropdown:border-acc-line group-has-[:popover-open]/icon-dropdown:bg-acc-soft group-has-[:popover-open]/icon-dropdown:text-acc",
                popovertarget: trigger_target,
                popovertargetaction: "toggle",
                aria_label: trigger_label.clone(),
                aria_haspopup: "dialog",
                aria_controls: id.clone(),
                title: trigger_label,
                "data-testid": trigger_test_id,
                span {
                    class: "transition-transform duration-150 ease-out group-has-[:popover-open]/icon-dropdown:rotate-90 motion-reduce:transition-none",
                    aria_hidden: "true",
                    Ellipsis { size: 18 }
                }
            }
            div {
                id,
                class: PANEL_CLASSES,
                popover: "auto",
                role: "dialog",
                aria_label,
                {children}
            }
        }
    }
}
