use super::{GuidedTourAnchor, layout::GuidedTourLayout};

#[cfg(target_arch = "wasm32")]
pub(super) fn measure(anchor: Option<GuidedTourAnchor>, scroll: bool) -> GuidedTourLayout {
    use wasm_bindgen::JsCast as _;

    use super::layout::{GuidedTourRect, GuidedTourViewport, guided_tour_layout};

    let Some(window) = web_sys::window() else {
        return GuidedTourLayout::default();
    };
    let Some(document) = window.document() else {
        return GuidedTourLayout::default();
    };
    let Some(root) = document.document_element() else {
        return GuidedTourLayout::default();
    };
    let viewport = GuidedTourViewport {
        width_px: f64::from(root.client_width()),
        height_px: f64::from(root.client_height()),
    };
    let target = anchor
        .and_then(|anchor| {
            document
                .query_selector_all(&format!(r#"[data-tour="{}"]"#, anchor.value()))
                .ok()
        })
        .and_then(|elements| {
            (0..elements.length())
                .filter_map(|index| elements.item(index)?.dyn_into::<web_sys::Element>().ok())
                .find(|element| {
                    let rect = element.get_bounding_client_rect();
                    element
                        .closest("[hidden], [aria-hidden='true']")
                        .ok()
                        .flatten()
                        .is_none()
                        && rect.width() > 0.0
                        && rect.height() > 0.0
                        && window
                            .get_computed_style(element)
                            .ok()
                            .flatten()
                            .is_some_and(|style| {
                                style
                                    .get_property_value("visibility")
                                    .is_ok_and(|value| value == "visible")
                            })
                })
        })
        .map(|element| {
            if scroll {
                let options = web_sys::ScrollIntoViewOptions::new();
                options.set_behavior(web_sys::ScrollBehavior::Instant);
                options.set_block(web_sys::ScrollLogicalPosition::Nearest);
                options.set_inline(web_sys::ScrollLogicalPosition::Nearest);
                element.scroll_into_view_with_scroll_into_view_options(&options);
            }
            let rect = element.get_bounding_client_rect();
            GuidedTourRect {
                left_px: rect.left(),
                top_px: rect.top(),
                width_px: rect.width(),
                height_px: rect.height(),
            }
        });
    guided_tour_layout(target, viewport)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn measure(_anchor: Option<GuidedTourAnchor>, _scroll: bool) -> GuidedTourLayout {
    GuidedTourLayout::default()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn move_focus(id: &str, backwards: bool) {
    use wasm_bindgen::JsCast as _;

    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(dialog) = document.get_element_by_id(id) else {
        return;
    };
    let Ok(elements) = dialog.query_selector_all("button:not([disabled]), [tabindex='0']") else {
        return;
    };
    let elements = (0..elements.length())
        .filter_map(|index| {
            elements
                .item(index)?
                .dyn_into::<web_sys::HtmlElement>()
                .ok()
        })
        .collect::<Vec<_>>();
    if elements.is_empty() {
        return;
    }
    let current = document.active_element().and_then(|active| {
        elements
            .iter()
            .position(|element| element.is_same_node(Some(&active)))
    });
    let next = if backwards {
        current.map_or(elements.len() - 1, |index| {
            (index + elements.len() - 1) % elements.len()
        })
    } else {
        current.map_or(0, |index| (index + 1) % elements.len())
    };
    let _ = elements[next].focus();
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn move_focus(_id: &str, _backwards: bool) {}
