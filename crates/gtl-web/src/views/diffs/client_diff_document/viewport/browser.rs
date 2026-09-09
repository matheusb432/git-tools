use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::{JsCast, closure::Closure};

use super::geometry::PinnedRegion;
use crate::views::diffs::presentation::ScrollAnchor;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ViewportMetrics {
    pub(super) top: f64,
    pub(super) height: f64,
    pub(super) width: f64,
    pub(super) anchor: Option<ScrollAnchor>,
    pub(super) pins: Vec<PinnedRegion>,
}

impl Default for ViewportMetrics {
    fn default() -> Self {
        Self {
            top: 0.0,
            height: 900.0,
            width: 0.0,
            anchor: None,
            pins: Vec::new(),
        }
    }
}

#[derive(Default)]
struct BrowserState {
    root: Option<web_sys::Element>,
    frame: Option<i32>,
    anchor: Option<ScrollAnchor>,
    search: Option<(usize, usize)>,
}

type Frame = Rc<Closure<dyn FnMut()>>;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct ViewportBrowser {
    state: Signal<BrowserState>,
    pub(super) metrics: ReadSignal<ViewportMetrics>,
    pub(super) refresh: Callback<()>,
}

impl ViewportBrowser {
    pub(super) fn mount(mut self, event: &MountedEvent) {
        if let Some(element) = event.data().downcast::<web_sys::Element>() {
            self.state.write().root = Some(element.clone());
            self.refresh.call(());
        }
    }

    pub(super) fn scroll_to(self, top: f64) {
        if let Some(root) = &self.state.peek().root {
            root.scroll_to_with_x_and_y(0.0, top.max(0.0));
        }
        self.refresh.call(());
    }

    pub(super) fn search(mut self, target: Option<(usize, usize)>) {
        self.state.write().search = target;
        if target.is_none() {
            crate::shared::browser::clear_diff_search_match();
        }
        self.refresh.call(());
    }

    pub(super) fn restore(mut self, anchor: ScrollAnchor) {
        self.state.write().anchor = Some(anchor);
        self.refresh.call(());
    }
}

pub(super) fn use_viewport_browser(initial_top: f64) -> ViewportBrowser {
    let mut state = use_signal(BrowserState::default);
    let metrics = use_signal(move || ViewportMetrics {
        top: initial_top,
        ..ViewportMetrics::default()
    });
    let frame = use_hook(move || -> Option<Frame> {
        if cfg!(target_arch = "wasm32") {
            Some(Rc::new(Closure::wrap(
                Box::new(move || paint(state, metrics)) as Box<dyn FnMut()>,
            )))
        } else {
            None
        }
    });
    let refresh = use_callback(move |()| {
        let Some(frame) = &frame else { return };
        let mut state = state.write();
        if state.frame.is_none()
            && let Some(window) = web_sys::window()
        {
            state.frame = window
                .request_animation_frame(frame.as_ref().as_ref().unchecked_ref())
                .ok();
        }
    });
    use_selection_changes(refresh);
    use_drop(move || {
        if let Some(frame) = state.write().frame.take()
            && let Some(window) = web_sys::window()
        {
            let _ = window.cancel_animation_frame(frame);
        }
    });
    ViewportBrowser {
        state,
        metrics: metrics.into(),
        refresh,
    }
}

fn paint(mut state: Signal<BrowserState>, mut metrics: Signal<ViewportMetrics>) {
    let (root, anchor) = {
        let mut state = state.write();
        state.frame = None;
        (state.root.clone(), state.anchor.clone())
    };
    let Some(root) = root.filter(|root| root.is_connected()) else {
        return;
    };
    if let Some(anchor) = anchor
        && align(&root, &anchor)
    {
        state.write().anchor = None;
    }
    if let Some((file, row)) = state.peek().search {
        crate::shared::browser::highlight_diff_search_match(file, row);
    }
    let next = ViewportMetrics {
        top: f64::from(root.scroll_top()),
        height: f64::from(root.client_height()),
        width: f64::from(root.client_width()),
        pins: pinned_regions(&root),
        anchor: if state.peek().anchor.is_none() {
            read_anchor(&root)
        } else {
            None
        },
    };
    if *metrics.peek() != next {
        metrics.set(next);
    }
}

fn use_selection_changes(refresh: Callback<()>) {
    let _listener = dioxus::dioxus_core::use_hook_with_cleanup(
        move || {
            if !cfg!(target_arch = "wasm32") {
                return None;
            }
            let document = web_sys::window()?.document()?;
            let callback = Rc::new(Closure::wrap(
                Box::new(move || refresh.call(())) as Box<dyn FnMut()>
            ));
            document
                .add_event_listener_with_callback(
                    "selectionchange",
                    callback.as_ref().as_ref().unchecked_ref(),
                )
                .ok()?;
            Some((document, callback))
        },
        |listener| {
            if let Some((document, callback)) = listener {
                let _ = document.remove_event_listener_with_callback(
                    "selectionchange",
                    callback.as_ref().as_ref().unchecked_ref(),
                );
            }
        },
    );
}

fn pinned_regions(root: &web_sys::Element) -> Vec<PinnedRegion> {
    let mut pins = Vec::with_capacity(3);
    let selection = web_sys::window()
        .and_then(|window| window.get_selection().ok())
        .flatten()
        .filter(|selection| !selection.is_collapsed());
    let nodes = selection
        .into_iter()
        .flat_map(|selection| [selection.anchor_node(), selection.focus_node()])
        .flatten();
    for node in nodes {
        if let Some(pin) = pinned_node(root, &node)
            && !pins.contains(&pin)
        {
            pins.push(pin);
        }
    }
    if let Some(element) = root
        .owner_document()
        .and_then(|document| document.active_element())
        && let Some(pin) = pinned_node(root, &element)
        && !pins.contains(&pin)
    {
        pins.push(pin);
    }
    pins
}

fn pinned_node(root: &web_sys::Element, node: &web_sys::Node) -> Option<PinnedRegion> {
    if !root.contains(Some(node)) {
        return None;
    }
    let element = node
        .dyn_ref::<web_sys::Element>()
        .cloned()
        .or_else(|| node.parent_element())?;
    let file = element
        .closest("[data-file-index]")
        .ok()??
        .get_attribute("data-file-index")?
        .parse()
        .ok()?;
    let window = element
        .closest("[data-gtl-row-window]")
        .ok()?
        .and_then(|window| window.get_attribute("data-gtl-row-window")?.parse().ok());
    Some(PinnedRegion { file, window })
}

fn align(root: &web_sys::Element, anchor: &ScrollAnchor) -> bool {
    let Some(file) = matching_file(root, &anchor.file) else {
        return false;
    };
    let target = match anchor.row {
        Some(row) => file
            .query_selector(&format!("[data-row-index='{row}']"))
            .ok()
            .flatten(),
        None => Some(file),
    };
    let Some(target) = target else { return false };
    let delta = target.get_bounding_client_rect().top() - root.get_bounding_client_rect().top()
        + anchor.offset;
    if delta.abs() > 0.5 {
        root.scroll_by_with_x_and_y(0.0, delta);
    }
    true
}

fn matching_file(root: &web_sys::Element, path: &str) -> Option<web_sys::Element> {
    let files = root.query_selector_all("[data-gtl-diff-file]").ok()?;
    (0..files.length())
        .filter_map(|index| files.item(index)?.dyn_into::<web_sys::Element>().ok())
        .find(|file| file.get_attribute("data-path").as_deref() == Some(path))
}

fn read_anchor(root: &web_sys::Element) -> Option<ScrollAnchor> {
    let top = root.get_bounding_client_rect().top();
    let rows = root.query_selector_all("[data-row-index]").ok()?;
    for index in 0..rows.length() {
        let row = rows.item(index)?.dyn_into::<web_sys::Element>().ok()?;
        let rect = row.get_bounding_client_rect();
        if rect.bottom() <= top {
            continue;
        }
        let file = row.closest("[data-gtl-diff-file]").ok()??;
        return Some(ScrollAnchor {
            file: file.get_attribute("data-path")?,
            row: Some(row.get_attribute("data-row-index")?.parse().ok()?),
            offset: top - rect.top(),
        });
    }
    let files = root.query_selector_all("[data-gtl-diff-file]").ok()?;
    for index in 0..files.length() {
        let file = files.item(index)?.dyn_into::<web_sys::Element>().ok()?;
        let rect = file.get_bounding_client_rect();
        if rect.bottom() > top {
            return Some(ScrollAnchor {
                file: file.get_attribute("data-path")?,
                row: None,
                offset: top - rect.top(),
            });
        }
    }
    None
}
