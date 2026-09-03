use std::time::Duration;

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlDialogElement, HtmlElement};

struct DialogState {
    id: String,
    trigger_id: String,
    open: bool,
    restore_focus: bool,
}

pub(super) fn use_dialog(id: &String, trigger_id: &String, open: bool) {
    let mut was_open = use_signal(|| false);
    let mut sync = use_action(move |state: DialogState| async move {
        sync_dialog_state(state).await;
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
            });
        },
    ));
}

async fn sync_dialog_state(state: DialogState) {
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
            focus_initial_element(&dialog).await;
        }
    } else if !state.open {
        if dialog.open() {
            dialog.close();
        }
        if state.restore_focus {
            restore_trigger_focus(&document, &state.trigger_id).await;
        }
    }
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
