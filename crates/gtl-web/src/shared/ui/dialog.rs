use std::time::Duration;

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlDialogElement, HtmlElement};

use super::animation::computed_animation_duration;

const DIALOG_SURFACE_SELECTOR: &str = "[data-dialog-surface]";

struct DialogState {
    id: String,
    trigger_id: String,
    open: bool,
    restore_focus: bool,
    close_duration_fallback: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DialogPhase {
    Closed,
    Open,
    Closing,
}

impl DialogPhase {
    #[cfg(feature = "interactive-ui")]
    #[cfg_attr(
        not(feature = "component-preview"),
        allow(dead_code, reason = "reserved for the viewer push feature")
    )]
    pub(super) const fn value(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Open => "open",
            Self::Closing => "closing",
        }
    }
}

pub(super) fn use_dialog(
    id: &String,
    trigger_id: &String,
    open: bool,
    close_duration_fallback: Duration,
) -> Signal<DialogPhase> {
    let phase = use_signal(|| DialogPhase::Closed);
    let mut was_open = use_signal(|| false);
    let mut sync = use_action(move |state: DialogState| async move {
        sync_dialog_state(state, phase).await;
        Ok::<(), std::convert::Infallible>(())
    });
    use_effect(use_reactive(
        (id, trigger_id, &open),
        move |(id, trigger_id, open)| {
            let restore_focus = *was_open.peek() && !open;
            was_open.set(open);
            sync.call(DialogState {
                id,
                trigger_id,
                open,
                restore_focus,
                close_duration_fallback,
            });
        },
    ));
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

    if state.open && !dialog.open() {
        if dialog.show_modal().is_ok() {
            phase.set(DialogPhase::Open);
            focus_initial_element(&dialog).await;
        }
    } else if state.open {
        phase.set(DialogPhase::Open);
    } else if !state.open {
        if dialog.open() {
            phase.set(DialogPhase::Closing);
            let close_duration =
                dialog_close_duration(&dialog, state.close_duration_fallback).await;
            dioxus_sdk_time::sleep(close_duration).await;
            dialog.close();
        }
        phase.set(DialogPhase::Closed);
        if state.restore_focus {
            restore_trigger_focus(&document, &state.trigger_id).await;
        }
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
        .query_selector("[data-dialog-initial-focus]")
        .ok()
        .flatten()
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
