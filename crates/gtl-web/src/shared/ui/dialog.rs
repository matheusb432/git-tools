use std::time::Duration;

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlDialogElement, HtmlElement};

use super::animation::computed_animation_duration;
use crate::shared::browser;

const DIALOG_SURFACE_SELECTOR: &str = "[data-dialog-surface]";

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum DialogPlacement {
    Center,
    NearTrigger,
}

struct DialogState {
    id: String,
    trigger_id: String,
    open: bool,
    restore_focus: bool,
    close_duration_fallback: Duration,
    placement: DialogPlacement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogPhase {
    Closed,
    Open,
    Closing,
}

impl DialogPhase {
    pub(crate) const fn value(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
            Self::Closing => "closing",
        }
    }
}

pub(crate) fn use_dialog(
    id: &String,
    trigger_id: &String,
    open: bool,
    close_duration_fallback: Duration,
    placement: DialogPlacement,
) -> Signal<DialogPhase> {
    let phase = use_signal(|| DialogPhase::Closed);
    let mut was_open = use_signal(|| false);
    let mut sync = use_action(move |state: DialogState| async move {
        sync_dialog_state(state, phase).await;
        Ok::<(), std::convert::Infallible>(())
    });
    use_effect(use_reactive(
        (id, trigger_id, &open, &placement),
        move |(id, trigger_id, open, placement)| {
            let restore_focus = *was_open.peek() && !open;
            was_open.set(open);
            sync.call(DialogState {
                id,
                trigger_id,
                open,
                restore_focus,
                close_duration_fallback,
                placement,
            });
        },
    ));
    let id = id.clone();
    let trigger_id = trigger_id.clone();
    let reposition = use_callback(move |()| {
        if open && placement == DialogPlacement::NearTrigger {
            position_dialog_near_trigger(&id, &trigger_id);
        }
    });
    browser::use_window_resize(move || reposition.call(()));
    phase
}

async fn sync_dialog_state(state: DialogState, mut phase: Signal<DialogPhase>) {
    dioxus_sdk_time::sleep(Duration::ZERO).await;
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(dialog) = document
        .get_element_by_id(&state.id)
        .and_then(|element| element.dyn_into::<HtmlDialogElement>().ok())
    else {
        return;
    };

    if state.open {
        let opening = !dialog.open();
        if opening && dialog.show_modal().is_err() {
            return;
        }
        if state.placement == DialogPlacement::NearTrigger {
            position_dialog_near_trigger(&state.id, &state.trigger_id);
        }
        phase.set(DialogPhase::Open);
        if opening {
            focus_initial_element(&dialog).await;
        }
        return;
    }
    if dialog.open() {
        phase.set(DialogPhase::Closing);
        let close_duration = dialog_close_duration(&dialog, state.close_duration_fallback).await;
        dioxus_sdk_time::sleep(close_duration).await;
        dialog.close();
    }
    phase.set(DialogPhase::Closed);
    if state.restore_focus {
        restore_trigger_focus(&document, &state.trigger_id).await;
    }
}

fn position_dialog_near_trigger(id: &str, trigger_id: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(dialog) = document
        .get_element_by_id(id)
        .and_then(|element| element.dyn_into::<HtmlDialogElement>().ok())
        .filter(HtmlDialogElement::open)
    else {
        return;
    };
    let Some(trigger) = document.get_element_by_id(trigger_id) else {
        return;
    };
    let Some(viewport) = document.document_element() else {
        return;
    };
    let width = f64::from(viewport.client_width());
    let height = f64::from(viewport.client_height());
    let trigger = trigger.get_bounding_client_rect();
    let style = dialog.style();
    let _ = style.remove_property("max-height");
    let bounds = dialog.get_bounding_client_rect();
    let dialog_width = bounds.width();
    let margin = 12.0;
    let gap = 8.0;
    let left = (trigger.right() - dialog_width)
        .max(margin)
        .min((width - dialog_width - margin).max(margin));
    let above = trigger.top() > height - trigger.bottom();
    let available = if above {
        trigger.top()
    } else {
        height - trigger.bottom()
    } - gap
        - margin;
    let (left, above, offset) = if available < bounds.height() || trigger.width() == 0.0 {
        (
            ((width - dialog_width) / 2.0).max(margin),
            true,
            ((height - bounds.height()) / 2.0).max(margin),
        )
    } else if above {
        (left, true, height - trigger.top() + gap)
    } else {
        (left, false, trigger.bottom() + gap)
    };
    let available = height - offset - margin;
    let _ = style.set_property("margin", "0");
    let _ = style.set_property("left", &format!("{left}px"));
    let _ = style.set_property("right", "auto");
    let _ = style.set_property("max-height", &format!("{available}px"));
    if above {
        let _ = style.set_property("top", "auto");
        let _ = style.set_property("bottom", &format!("{offset}px"));
    } else {
        let _ = style.set_property("bottom", "auto");
        let _ = style.set_property("top", &format!("{offset}px"));
    }
}

async fn dialog_close_duration(
    dialog: &HtmlDialogElement,
    close_duration_fallback: Duration,
) -> Duration {
    if close_duration_fallback.is_zero() {
        return Duration::ZERO;
    }

    dioxus_sdk_time::sleep(Duration::ZERO).await;
    dialog
        .query_selector(DIALOG_SURFACE_SELECTOR)
        .ok()
        .flatten()
        .and_then(|surface| computed_animation_duration(&surface))
        .unwrap_or(close_duration_fallback)
}

async fn focus_initial_element(dialog: &HtmlDialogElement) {
    dioxus_sdk_time::sleep(Duration::ZERO).await;
    let initial_focus = dialog
        .query_selector("[data-dialog-content-initial-focus]")
        .ok()
        .flatten()
        .or_else(|| {
            dialog
                .query_selector("[data-dialog-initial-focus]")
                .ok()
                .flatten()
        })
        .and_then(|element| element.dyn_into::<HtmlElement>().ok());
    focus_element(initial_focus);
}

async fn restore_trigger_focus(document: &web_sys::Document, trigger_id: &str) {
    dioxus_sdk_time::sleep(Duration::ZERO).await;
    let trigger = document
        .get_element_by_id(trigger_id)
        .and_then(|element| element.dyn_into::<HtmlElement>().ok());
    focus_element(trigger);
}

fn focus_element(element: Option<HtmlElement>) {
    if let Some(element) = element {
        let _ = element.focus();
    }
}
