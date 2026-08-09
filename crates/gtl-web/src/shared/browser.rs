use dioxus::prelude::{document, spawn};
use serde::Serialize;

use super::bridge::ClientApiError;

const APPLY_THEME_SCRIPT: &str = r"
const theme = await dioxus.recv();
document.documentElement.dataset.theme = theme;
return null;
";

const FOCUS_ELEMENT_SCRIPT: &str = r"
const id = await dioxus.recv();
requestAnimationFrame(() => document.getElementById(id)?.focus());
return null;
";

const COPY_JSON_SCRIPT: &str = r"
const value = await dioxus.recv();
const text = JSON.stringify(value, null, 2);
if (navigator.clipboard?.writeText) {
    try {
        await navigator.clipboard.writeText(text);
        return null;
    } catch (_error) {
        // Fall through to the document command for WebViews without Clipboard API permission.
    }
}
const textarea = document.createElement('textarea');
textarea.value = text;
textarea.style.position = 'fixed';
textarea.style.opacity = '0';
document.body.appendChild(textarea);
textarea.select();
const copied = document.execCommand('copy');
textarea.remove();
if (!copied) throw new Error('clipboard unavailable');
return null;
";

pub(crate) fn apply_theme(theme: &'static str) {
    spawn(async move {
        let evaluator = document::eval(APPLY_THEME_SCRIPT);
        if evaluator.send(theme).is_ok() {
            let _ = evaluator.join::<()>().await;
        }
    });
}

pub(crate) fn focus_element(id: String) {
    spawn(async move {
        let evaluator = document::eval(FOCUS_ELEMENT_SCRIPT);
        if evaluator.send(id).is_ok() {
            let _ = evaluator.join::<()>().await;
        }
    });
}

pub(crate) async fn copy_json(value: &impl Serialize) -> Result<(), ClientApiError> {
    let evaluator = document::eval(COPY_JSON_SCRIPT);
    evaluator
        .send(value)
        .map_err(|_| ClientApiError::Unavailable)?;
    evaluator
        .join::<()>()
        .await
        .map_err(|_| ClientApiError::Unavailable)
}
