#[cfg(feature = "desktop")]
use std::time::Duration;

#[cfg(feature = "desktop")]
use dioxus::prelude::spawn;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
#[cfg(feature = "desktop")]
use web_sys::{HtmlDetailsElement, HtmlElement};
use web_sys::{HtmlDocument, HtmlTextAreaElement};

#[cfg(feature = "desktop")]
pub(crate) fn apply_theme(theme: &'static str) {
    let Some(root) = document().and_then(|document| document.document_element()) else {
        return;
    };
    let _ = root.set_attribute("data-theme", theme);
}

#[cfg(feature = "desktop")]
pub(crate) fn focus_element(id: String) {
    spawn(async move {
        dioxus_sdk_time::sleep(Duration::ZERO).await;
        let Some(element) = document()
            .and_then(|document| document.get_element_by_id(&id))
            .and_then(|element| element.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        let _ = element.focus();
    });
}

#[cfg(feature = "desktop")]
pub(crate) fn hide_popover(id: &str) {
    let Some(element) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let _ = element.hide_popover();
}

#[cfg(feature = "desktop")]
pub(crate) fn scroll_to_file(id: &str) {
    let Some(details) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlDetailsElement>().ok())
    else {
        return;
    };
    details.set_open(true);
    details.scroll_into_view_with_bool(true);
}

pub(crate) async fn copy_text(text: &str) -> bool {
    if let Some(window) = web_sys::window()
        && JsFuture::from(window.navigator().clipboard().write_text(text))
            .await
            .is_ok()
    {
        return true;
    }
    exec_copy(text)
}

fn exec_copy(text: &str) -> bool {
    let Some(document) = document().and_then(|document| document.dyn_into::<HtmlDocument>().ok())
    else {
        return false;
    };
    let Some(body) = document.body() else {
        return false;
    };
    let Ok(element) = document.create_element("textarea") else {
        return false;
    };
    let Ok(textarea) = element.dyn_into::<HtmlTextAreaElement>() else {
        return false;
    };
    textarea.set_value(text);
    let _ = textarea.style().set_property("position", "fixed");
    let _ = textarea.style().set_property("opacity", "0");
    if body.append_child(&textarea).is_err() {
        return false;
    }
    textarea.select();
    let copied = document.exec_command("copy").unwrap_or(false);
    let _ = body.remove_child(&textarea);
    copied
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}
