use std::rc::Rc;

use dioxus::{html::input_data::MouseButton, prelude::*};
use gtl_models::viewer::ViewerTabId;
use gtl_wire::viewer::MoveViewerTab;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{HtmlElement, Window};

use super::{PointerDrag, TabDragLayout, TabSlot};

const CANCEL_EVENTS: [&str; 3] = ["blur", "resize", "scroll"];
const DRAG_THRESHOLD_PX: f64 = 5.0;
const FLOATING_CLASSES: &str = "viewer-tab-floating m-0 min-w-0 max-w-none p-0";

#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    fn coordinate(self, point: Point) -> f64 {
        match self {
            Self::Horizontal => point.x,
            Self::Vertical => point.y,
        }
    }
}

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn from_event(event: &PointerEvent) -> Self {
        let point = event.client_coordinates();
        Self {
            x: point.x,
            y: point.y,
        }
    }
}

struct DragSession {
    window: Window,
    cancel_listener: Rc<Closure<dyn FnMut(web_sys::Event)>>,
    handle: HtmlElement,
    pointer_id: i32,
    elements: Vec<HtmlElement>,
    layout: TabDragLayout,
    axis: Axis,
    origin: Point,
    latest: Point,
    bounds: web_sys::DomRect,
    floating: Option<HtmlElement>,
    destination: usize,
}

impl DragSession {
    fn capture(
        event: &PointerEvent,
        cancel_listener: Rc<Closure<dyn FnMut(web_sys::Event)>>,
    ) -> Option<Self> {
        let window = web_sys::window()?;
        let target = event
            .data()
            .downcast::<web_sys::PointerEvent>()?
            .target()?
            .dyn_into::<web_sys::Element>()
            .ok()?;
        let handle = target
            .closest("button")
            .ok()??
            .dyn_into::<HtmlElement>()
            .ok()?;
        let source = handle.parent_element()?.dyn_into::<HtmlElement>().ok()?;
        let axis = match source.get_attribute("data-viewer-tab-axis")?.as_str() {
            "horizontal" => Axis::Horizontal,
            "vertical" => Axis::Vertical,
            _ => return None,
        };
        let tab_id = source
            .get_attribute("data-viewer-tab-id")?
            .parse()
            .ok()
            .and_then(|value| ViewerTabId::try_new(value).ok())?;
        let nodes = source
            .parent_element()?
            .query_selector_all(":scope > [data-viewer-tab-id]")
            .ok()?;
        let mut elements = Vec::new();
        let mut slots = Vec::new();
        for index in 0..nodes.length() {
            let element = nodes.item(index)?.dyn_into::<HtmlElement>().ok()?;
            let id = element
                .get_attribute("data-viewer-tab-id")?
                .parse()
                .ok()
                .and_then(|value| ViewerTabId::try_new(value).ok())?;
            let bounds = element.get_bounding_client_rect();
            let (start, end) = match axis {
                Axis::Horizontal => (bounds.left(), bounds.right()),
                Axis::Vertical => (bounds.top(), bounds.bottom()),
            };
            slots.push(TabSlot { id, start, end });
            elements.push(element);
        }
        let layout = TabDragLayout::new(slots, tab_id)?;
        let destination = layout.source;
        let bounds = source.get_bounding_client_rect();
        let pointer_id = event.pointer_id();
        handle.set_pointer_capture(pointer_id).ok()?;
        let session = Self {
            window,
            cancel_listener,
            handle,
            pointer_id,
            elements,
            layout,
            axis,
            origin: Point::from_event(event),
            latest: Point::from_event(event),
            bounds,
            floating: None,
            destination,
        };
        for name in CANCEL_EVENTS {
            session
                .window
                .add_event_listener_with_callback_and_bool(
                    name,
                    session.cancel_listener.as_ref().as_ref().unchecked_ref(),
                    true,
                )
                .ok()?;
        }
        Some(session)
    }

    fn distance(&self) -> f64 {
        self.axis.coordinate(self.latest) - self.axis.coordinate(self.origin)
    }

    fn lift(&mut self) -> Option<()> {
        let source = &self.elements[self.layout.source];
        let floating = source
            .clone_node_with_deep(true)
            .ok()?
            .dyn_into::<HtmlElement>()
            .ok()?;
        floating.set_class_name(FLOATING_CLASSES);
        floating.remove_attribute("data-viewer-tab-id").ok()?;
        floating.remove_attribute("data-viewer-tab-axis").ok()?;
        floating
            .set_attribute("data-viewer-tab-floating", "true")
            .ok()?;
        floating.set_attribute("aria-hidden", "true").ok()?;
        floating.set_attribute("inert", "").ok()?;
        floating.set_attribute("popover", "manual").ok()?;
        floating.remove_attribute("id").ok()?;
        let descendants = floating
            .query_selector_all("[id], [data-dioxus-id], [role], [title], [data-testid]")
            .ok()?;
        for index in 0..descendants.length() {
            let element = descendants
                .item(index)?
                .dyn_into::<web_sys::Element>()
                .ok()?;
            element.remove_attribute("id").ok()?;
            element.remove_attribute("data-dioxus-id").ok()?;
            element.remove_attribute("role").ok()?;
            element.remove_attribute("title").ok()?;
            element.remove_attribute("data-testid").ok()?;
        }
        let style = floating.style();
        style
            .set_property("width", &format!("{}px", self.bounds.width()))
            .ok()?;
        style
            .set_property("height", &format!("{}px", self.bounds.height()))
            .ok()?;
        style.set_property("left", "0").ok()?;
        style.set_property("top", "0").ok()?;
        source.parent_element()?.append_child(&floating).ok()?;
        self.floating = Some(floating.clone());
        floating.show_popover().ok()?;
        for element in &self.elements {
            let _ = element.set_attribute("data-drag-state", "shifting");
        }
        let _ = source.set_attribute("data-drag-state", "dragging");
        Some(())
    }

    fn paint(&mut self) -> Option<()> {
        if !self.handle.is_connected() {
            return None;
        }
        let x = self.latest.x - self.origin.x;
        let y = self.latest.y - self.origin.y;
        if self.floating.is_none() && x.hypot(y) < DRAG_THRESHOLD_PX {
            return Some(());
        }
        if self.floating.is_none() {
            self.lift()?;
        }
        let floating = self.floating.as_ref()?;
        let _ = floating.style().set_property(
            "transform",
            &format!(
                "translate3d({}px, {}px, 0)",
                self.bounds.left() + x,
                self.bounds.top() + y,
            ),
        );
        let destination = self.layout.destination(self.distance());
        if self.destination == destination {
            return Some(());
        }
        self.destination = destination;
        for (element, offset) in self
            .elements
            .iter()
            .zip(self.layout.offsets(self.distance()))
        {
            let transform = match self.axis {
                Axis::Horizontal => format!("translate3d({offset}px, 0, 0)"),
                Axis::Vertical => format!("translate3d(0, {offset}px, 0)"),
            };
            let _ = element.style().set_property("transform", &transform);
        }
        Some(())
    }
}

impl Drop for DragSession {
    fn drop(&mut self) {
        for name in CANCEL_EVENTS {
            let _ = self.window.remove_event_listener_with_callback_and_bool(
                name,
                self.cancel_listener.as_ref().as_ref().unchecked_ref(),
                true,
            );
        }
        if let Some(floating) = self.floating.take() {
            let _ = floating.hide_popover();
            floating.remove();
        }
        for element in &self.elements {
            let _ = element.remove_attribute("data-drag-state");
            let _ = element.style().remove_property("transform");
        }
        if self.handle.has_pointer_capture(self.pointer_id) {
            let _ = self.handle.release_pointer_capture(self.pointer_id);
        }
    }
}

#[derive(Default)]
struct DragState {
    session: Option<DragSession>,
    frame_request: Option<i32>,
    suppress_click: bool,
}

fn cancel(mut state: Signal<DragState>) {
    let session = {
        let mut state = state.write();
        if let Some(request) = state.frame_request.take()
            && let Some(window) = web_sys::window()
        {
            let _ = window.cancel_animation_frame(request);
        }
        if let Some(session) = &state.session {
            state.suppress_click |= session.floating.is_some();
        }
        state.session.take()
    };
    drop(session);
}

fn paint_frame(mut state: Signal<DragState>) {
    let failed = {
        let mut state = state.write();
        state.frame_request = None;
        state
            .session
            .as_mut()
            .is_some_and(|session| session.paint().is_none())
    };
    if failed {
        cancel(state);
    }
}

fn cancel_on_event(state: Signal<DragState>, event: &web_sys::Event) {
    // Routed content emits element resize events without changing the captured tab layout.
    if !matches!(event.type_().as_str(), "blur" | "resize")
        || event
            .target()
            .is_some_and(|target| target.is_instance_of::<Window>())
    {
        cancel(state);
    }
}

fn release_request(state: &mut DragState, event: &PointerEvent) -> Option<MoveViewerTab> {
    let session = state.session.as_mut()?;
    if session.pointer_id != event.pointer_id() {
        return None;
    }
    session.latest = Point::from_event(event);
    session.paint()?;
    session.floating.as_ref()?;
    session.layout.request(session.distance())
}

pub(super) fn use_pointer_drag(onmove: Option<EventHandler<MoveViewerTab>>) -> PointerDrag {
    let mut state = use_signal(DragState::default);
    let frame = use_hook(move || {
        Rc::new(Closure::wrap(
            Box::new(move || paint_frame(state)) as Box<dyn FnMut()>
        ))
    });
    let cancel_listener = use_hook(move || {
        Rc::new(Closure::wrap(
            Box::new(move |event: web_sys::Event| cancel_on_event(state, &event))
                as Box<dyn FnMut(web_sys::Event)>,
        ))
    });
    use_drop(move || cancel(state));
    PointerDrag {
        start: use_callback(move |event: PointerEvent| {
            if event.trigger_button() != Some(MouseButton::Primary) || !event.is_primary() {
                return;
            }
            cancel(state);
            let session = DragSession::capture(&event, Rc::clone(&cancel_listener));
            let mut state = state.write();
            state.suppress_click = false;
            state.session = session;
        }),
        move_pointer: use_callback(move |event: PointerEvent| {
            let mut state = state.write();
            let Some(session) = &mut state.session else {
                return;
            };
            if session.pointer_id != event.pointer_id() {
                return;
            }
            session.latest = Point::from_event(&event);
            if state.frame_request.is_none()
                && let Some(window) = web_sys::window()
            {
                state.frame_request = window
                    .request_animation_frame(frame.as_ref().as_ref().unchecked_ref())
                    .ok();
            }
        }),
        release: use_callback(move |event: PointerEvent| {
            if state
                .peek()
                .session
                .as_ref()
                .is_none_or(|session| session.pointer_id != event.pointer_id())
            {
                return;
            }
            let request = release_request(&mut state.write(), &event);
            cancel(state);
            if let Some(request) = request
                && let Some(onmove) = onmove
            {
                onmove.call(request);
            }
        }),
        cancel: use_callback(move |()| cancel(state)),
        suppress_click: use_callback(move |()| std::mem::take(&mut state.write().suppress_click)),
    }
}
