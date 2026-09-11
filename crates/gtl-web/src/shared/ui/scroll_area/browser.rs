use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use super::{geometry::ScrollbarGeometry, scrollbar::ScrollAxis};

struct ScrollElements {
    viewport: HtmlElement,
    layer: HtmlElement,
    rails: Vec<ScrollbarElements>,
}

impl ScrollElements {
    fn from_layer(layer: HtmlElement, fallback_id: &str) -> Option<Self> {
        let document = layer.owner_document()?;
        let viewport: HtmlElement = match layer.get_attribute("data-scroll-target") {
            Some(id) => document.get_element_by_id(&id)?,
            None => layer.parent_element()?,
        }
        .dyn_into()
        .ok()?;
        if viewport.id().is_empty() {
            viewport.set_id(fallback_id);
        }
        let rails = [ScrollAxis::Horizontal, ScrollAxis::Vertical]
            .into_iter()
            .filter_map(|axis| {
                let rail: HtmlElement = layer
                    .query_selector(&format!("[data-scroll-axis='{}']", axis.as_str()))
                    .ok()??
                    .dyn_into()
                    .ok()?;
                let thumb = rail.first_element_child()?.dyn_into().ok()?;
                set_attribute(&rail, "aria-controls", &viewport.id());
                Some(ScrollbarElements { axis, rail, thumb })
            })
            .collect();
        set_attribute(&viewport, "data-scroll-enhanced", "true");
        Some(Self {
            viewport,
            layer,
            rails,
        })
    }

    fn scroll_to(&self, axis: ScrollAxis, position: f64) {
        match axis {
            ScrollAxis::Horizontal => self
                .viewport
                .scroll_to_with_x_and_y(position, f64::from(self.viewport.scroll_top())),
            ScrollAxis::Vertical => self
                .viewport
                .scroll_to_with_x_and_y(f64::from(self.viewport.scroll_left()), position),
        }
        self.refresh();
    }

    fn refresh(&self) {
        let width = self.viewport.client_width();
        let height = self.viewport.client_height();
        if width == 0 || height == 0 {
            return;
        }
        set_style(&self.layer, "--scroll-width", &format!("{width}px"));
        set_style(&self.layer, "--scroll-height", &format!("{height}px"));
        for rail in &self.rails {
            rail.refresh(&self.viewport);
        }
    }
}

struct Drag {
    axis: ScrollAxis,
    pointer_id: i32,
    pointer_start: f64,
    scroll_start: f64,
    geometry: ScrollbarGeometry,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct ScrollbarController {
    elements: Signal<Option<ScrollElements>>,
    drag: Signal<Option<Drag>>,
    scope_id: usize,
}

pub(crate) fn use_scrollbars() -> ScrollbarController {
    ScrollbarController {
        elements: use_signal(|| None),
        drag: use_signal(|| None),
        scope_id: dioxus::dioxus_core::current_scope_id().0,
    }
}

impl ScrollbarController {
    pub(crate) fn mount(mut self, event: &MountedEvent) {
        let Some(layer) = event
            .data()
            .downcast::<web_sys::Element>()
            .and_then(|element| element.dyn_ref::<HtmlElement>())
            .cloned()
        else {
            return;
        };
        self.elements.set(ScrollElements::from_layer(
            layer,
            &format!("scroll-viewport-{}", self.scope_id),
        ));
        self.measure();
    }

    pub(crate) fn measure(self) {
        let Some(elements) = &*self.elements.peek() else {
            return;
        };
        elements.refresh();
        let Some(style) = web_sys::window()
            .and_then(|window| window.get_computed_style(&elements.viewport).ok().flatten())
        else {
            return;
        };
        let left = style.get_property_value("padding-left").unwrap_or_default();
        let top = style.get_property_value("padding-top").unwrap_or_default();
        set_style(&elements.layer, "--scroll-left", &left);
        set_style(&elements.layer, "--scroll-top", &top);
    }

    pub(crate) fn refresh(self) {
        if let Some(elements) = &*self.elements.peek() {
            elements.refresh();
        }
    }

    pub(super) fn pointer_down(mut self, event: &PointerEvent) {
        let data = event.data();
        let Some(pointer) = data.downcast::<web_sys::PointerEvent>() else {
            return;
        };
        if pointer.button() != 0 || self.drag.peek().is_some() {
            return;
        }
        let Some(elements) = &*self.elements.peek() else {
            return;
        };
        let Some(target) = pointer
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        let Some(scrollbar) = elements
            .rails
            .iter()
            .find(|scrollbar| scrollbar.rail.contains(Some(&target)))
        else {
            return;
        };
        event.prevent_default();
        event.stop_propagation();
        let (viewport, content, position) = scroll_metrics(&elements.viewport, scrollbar.axis);
        let geometry = ScrollbarGeometry::new(viewport, content, position);
        let axis = scrollbar.axis;
        let rail = &scrollbar.rail;
        let pointer_start = pointer_position(pointer, axis);
        if !scrollbar.thumb.contains(Some(&target)) {
            let rect = rail.get_bounding_client_rect();
            let start = match axis {
                ScrollAxis::Horizontal => rect.x(),
                ScrollAxis::Vertical => rect.y(),
            };
            elements.scroll_to(
                axis,
                position + geometry.page_delta(pointer_start - start, viewport),
            );
            let _ = rail.focus();
            return;
        }
        let _ = rail.focus();
        let _ = elements.layer.set_pointer_capture(pointer.pointer_id());
        set_attribute(rail, "data-dragging", "true");
        self.drag.set(Some(Drag {
            axis,
            pointer_id: pointer.pointer_id(),
            pointer_start,
            scroll_start: position,
            geometry,
        }));
    }

    pub(super) fn pointer_move(self, event: &PointerEvent) {
        let data = event.data();
        let Some(pointer) = data.downcast::<web_sys::PointerEvent>() else {
            return;
        };
        let Some(drag) = &*self.drag.peek() else {
            return;
        };
        if pointer.pointer_id() != drag.pointer_id {
            return;
        }
        if let Some(elements) = &*self.elements.peek() {
            event.prevent_default();
            elements.scroll_to(
                drag.axis,
                drag.scroll_start
                    + drag
                        .geometry
                        .scroll_delta(pointer_position(pointer, drag.axis) - drag.pointer_start),
            );
        }
    }

    pub(super) fn pointer_end(mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let Some(elements) = &*self.elements.peek() else {
            return;
        };
        for scrollbar in &elements.rails {
            let _ = scrollbar.rail.remove_attribute("data-dragging");
        }
        let _ = elements.layer.release_pointer_capture(drag.pointer_id);
    }

    pub(super) fn keydown(self, event: &KeyboardEvent) {
        let Some(elements) = &*self.elements.peek() else {
            return;
        };
        let active = elements
            .viewport
            .owner_document()
            .and_then(|document| document.active_element());
        let Some(scrollbar) = elements
            .rails
            .iter()
            .find(|scrollbar| active.as_ref() == Some(scrollbar.rail.as_ref()))
        else {
            return;
        };
        let (viewport, content, position) = scroll_metrics(&elements.viewport, scrollbar.axis);
        let axis = scrollbar.axis;
        let target = match event.key() {
            Key::ArrowLeft if axis == ScrollAxis::Horizontal => position - 40.0,
            Key::ArrowRight if axis == ScrollAxis::Horizontal => position + 40.0,
            Key::ArrowUp if axis == ScrollAxis::Vertical => position - 40.0,
            Key::ArrowDown if axis == ScrollAxis::Vertical => position + 40.0,
            Key::PageUp => position - viewport,
            Key::PageDown => position + viewport,
            Key::Home => 0.0,
            Key::End => content - viewport,
            _ => return,
        };
        event.prevent_default();
        event.stop_propagation();
        elements.scroll_to(axis, target);
    }
}

fn pointer_position(pointer: &web_sys::PointerEvent, axis: ScrollAxis) -> f64 {
    match axis {
        ScrollAxis::Horizontal => f64::from(pointer.client_x()),
        ScrollAxis::Vertical => f64::from(pointer.client_y()),
    }
}

fn set_attribute(element: &HtmlElement, name: &str, value: &str) {
    if element.get_attribute(name).as_deref() != Some(value) {
        let _ = element.set_attribute(name, value);
    }
}

fn set_style(element: &HtmlElement, name: &str, value: &str) {
    if element.style().get_property_value(name).ok().as_deref() != Some(value) {
        let _ = element.style().set_property(name, value);
    }
}

fn scroll_metrics(viewport: &HtmlElement, axis: ScrollAxis) -> (f64, f64, f64) {
    match axis {
        ScrollAxis::Horizontal => (
            f64::from(viewport.client_width()),
            f64::from(viewport.scroll_width()),
            f64::from(viewport.scroll_left()),
        ),
        ScrollAxis::Vertical => (
            f64::from(viewport.client_height()),
            f64::from(viewport.scroll_height()),
            f64::from(viewport.scroll_top()),
        ),
    }
}

struct ScrollbarElements {
    axis: ScrollAxis,
    rail: HtmlElement,
    thumb: HtmlElement,
}
impl ScrollbarElements {
    fn refresh(&self, viewport_element: &HtmlElement) {
        let (viewport, content, position) = scroll_metrics(viewport_element, self.axis);
        let geometry = ScrollbarGeometry::new(viewport, content, position);
        let overflow = content > viewport;
        if self.axis == ScrollAxis::Horizontal {
            set_attribute(
                viewport_element,
                "data-scroll-overflow-x",
                &overflow.to_string(),
            );
        }
        set_attribute(
            &self.rail,
            "aria-hidden",
            if overflow { "false" } else { "true" },
        );
        set_attribute(&self.rail, "tabindex", if overflow { "0" } else { "-1" });
        set_attribute(
            &self.rail,
            "aria-valuemax",
            &format!("{}", (content - viewport).max(0.0)),
        );
        set_attribute(&self.rail, "aria-valuenow", &position.to_string());
        let (size, transform) = match self.axis {
            ScrollAxis::Horizontal => ("width", format!("translateX({}px)", geometry.thumb_offset)),
            ScrollAxis::Vertical => ("height", format!("translateY({}px)", geometry.thumb_offset)),
        };
        set_style(&self.thumb, size, &format!("{}px", geometry.thumb_size));
        set_style(&self.thumb, "transform", &transform);
    }
}
