use dioxus::prelude::{document, spawn};
use serde::Serialize;

const SYNC_DIALOG_SCRIPT: &str = r#"
const state = await dioxus.recv();
const dialog = document.getElementById(state.id);
if (!(dialog instanceof HTMLDialogElement)) return null;
if (state.open && !dialog.open) {
    dialog.showModal();
    requestAnimationFrame(() => {
        dialog.querySelector("[data-dialog-initial-focus]")?.focus();
    });
} else if (!state.open) {
    if (dialog.open) dialog.close();
    if (state.restoreFocus) {
        requestAnimationFrame(() => document.getElementById(state.triggerId)?.focus());
    }
}
return null;
"#;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DialogState {
    pub(super) id: String,
    pub(super) trigger_id: String,
    pub(super) open: bool,
    pub(super) restore_focus: bool,
}

pub(super) fn sync_dialog(state: DialogState) {
    spawn(async move {
        let evaluator = document::eval(SYNC_DIALOG_SCRIPT);
        if evaluator.send(state).is_ok() {
            let _ = evaluator.join::<()>().await;
        }
    });
}
