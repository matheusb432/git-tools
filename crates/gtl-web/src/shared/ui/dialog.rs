use std::time::Duration;

use dioxus::prelude::spawn;
use wasm_bindgen::JsCast;
use web_sys::{HtmlDialogElement, HtmlElement};

pub(super) struct DialogState {
    pub(super) id: String,
    pub(super) trigger_id: String,
    pub(super) open: bool,
    pub(super) restore_focus: bool,
}

pub(super) fn sync_dialog(state: DialogState) {
    spawn(async move {
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
                dioxus_sdk_time::sleep(Duration::ZERO).await;
                let initial_focus = dialog
                    .query_selector("[data-dialog-initial-focus]")
                    .ok()
                    .flatten()
                    .and_then(|element| element.dyn_into::<HtmlElement>().ok());
                if let Some(element) = initial_focus {
                    let _ = element.focus();
                }
            }
        } else if !state.open {
            if dialog.open() {
                dialog.close();
            }
            if state.restore_focus {
                dioxus_sdk_time::sleep(Duration::ZERO).await;
                let trigger = document
                    .get_element_by_id(&state.trigger_id)
                    .and_then(|element| element.dyn_into::<HtmlElement>().ok());
                if let Some(element) = trigger {
                    let _ = element.focus();
                }
            }
        }
    });
}
