use dioxus::prelude::*;
use gtl_models::viewer::ViewerTabId;

use crate::shared::i18n::{t, use_language};

#[component]
pub(crate) fn ViewerTabRail(active_tab_id: Option<ViewerTabId>, children: Element) -> Element {
    let language = use_language();
    let mut element = use_signal(|| None::<web_sys::Element>);
    let reveal = use_callback(move |()| {
        let root = element.peek();
        let Some(root) = root.as_ref() else { return };
        if let Some(id) = active_tab_id
            && let Ok(Some(tab)) = root.query_selector(&format!("[data-viewer-tab-id='{id}']"))
        {
            let viewport = root.get_bounding_client_rect();
            let bounds = tab.get_bounding_client_rect();
            let delta = if bounds.left() < viewport.left() {
                bounds.left() - viewport.left()
            } else if bounds.right() > viewport.right() {
                bounds.right() - viewport.right()
            } else {
                0.0
            };
            root.scroll_by_with_x_and_y(delta, 0.0);
        }
        update_edges(root);
    });
    use_effect(use_reactive((&active_tab_id,), move |_| {
        let _ = element.read();
        reveal.call(());
    }));
    rsx! {
        div {
            class: "viewer-tab-rail",
            role: "tablist",
            aria_label: t!(language, "navigation-open-diffs"),
            onmounted: move |event: MountedEvent| {
                element.set(event.data().downcast::<web_sys::Element>().cloned());
            },
            onresize: move |_| reveal.call(()),
            onscroll: move |_| {
                if let Some(root) = element.peek().as_ref() {
                    update_edges(root);
                }
            },
            onwheel: move |event: WheelEvent| {
                if event.modifiers().intersects(Modifiers::CONTROL | Modifiers::META) {
                    return;
                }
                let root = element.peek();
                let Some(root) = root.as_ref() else { return };
                if root.scroll_width() <= root.client_width() {
                    return;
                }
                let delta = event.delta();
                let unit = match delta {
                    dioxus::html::geometry::WheelDelta::Pixels(_) => 1.0,
                    dioxus::html::geometry::WheelDelta::Lines(_) => 36.0,
                    dioxus::html::geometry::WheelDelta::Pages(_) => {
                        f64::from(root.client_width())
                    }
                };
                let vector = delta.strip_units();
                let distance = if vector.x.abs() > vector.y.abs() { vector.x } else { vector.y };
                event.prevent_default();
                root.scroll_by_with_x_and_y(distance * unit, 0.0);
            },
            div {
                class: "w-max min-w-full",
                onresize: move |_| {
                    if let Some(root) = element.peek().as_ref() {
                        update_edges(root);
                    }
                },
                {children}
            }
        }
    }
}

fn update_edges(root: &web_sys::Element) {
    let left = root.scroll_left();
    let _ = root.set_attribute(
        "data-overflow-left",
        if left > 1 { "true" } else { "false" },
    );
    let _ = root.set_attribute(
        "data-overflow-right",
        if left + root.client_width() + 1 < root.scroll_width() {
            "true"
        } else {
            "false"
        },
    );
}
