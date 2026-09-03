#[cfg(feature = "desktop")]
use std::time::Duration;

#[cfg(feature = "desktop")]
use dioxus::prelude::spawn;
#[cfg(any(feature = "artifact", feature = "desktop"))]
use wasm_bindgen::JsCast;
#[cfg(any(feature = "artifact", feature = "desktop"))]
use wasm_bindgen_futures::JsFuture;
#[cfg(feature = "desktop")]
use web_sys::HtmlDetailsElement;
#[cfg(any(feature = "artifact", feature = "desktop"))]
use web_sys::{HtmlDocument, HtmlElement, HtmlTextAreaElement};

#[cfg(feature = "interactive-ui")]
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

#[cfg(any(feature = "artifact", feature = "desktop"))]
pub(crate) fn hide_popover(id: &str) {
    let Some(element) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let _ = element.hide_popover();
}

#[cfg(any(feature = "artifact", feature = "desktop"))]
pub(crate) fn show_popover(id: &str) {
    let Some(element) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let _ = element.show_popover();
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

#[cfg(any(feature = "artifact", feature = "desktop"))]
pub(crate) fn scroll_diff_document_to_start() {
    let Some(diff_document) = document()
        .and_then(|document| document.query_selector("[data-gtl-diff-document]").ok())
        .flatten()
    else {
        return;
    };
    diff_document.set_scroll_top(0);
}

#[cfg(feature = "desktop")]
pub(crate) fn show_diff_search_match(file_index: usize, row_index: usize) -> bool {
    clear_diff_search_match();
    let Some(container) = document()
        .and_then(|document| document.get_element_by_id(&format!("viewer-diff-{file_index}")))
    else {
        return false;
    };
    let Ok(rows) = container.query_selector_all("[data-gtl-diff-row]") else {
        return false;
    };
    let Ok(row_index) = u32::try_from(row_index) else {
        return false;
    };
    let Some(element) = rows
        .item(row_index)
        .and_then(|row| row.dyn_into::<HtmlElement>().ok())
    else {
        return false;
    };
    if let Ok(Some(details)) = element.closest("details")
        && let Ok(details) = details.dyn_into::<HtmlDetailsElement>()
    {
        details.set_open(true);
    }
    let style = element.style();
    if style
        .set_property("outline", "2px solid var(--color-acc)")
        .is_err()
    {
        return false;
    }
    if style.set_property("outline-offset", "-2px").is_err() {
        let _ = style.remove_property("outline");
        return false;
    }
    if element.set_attribute("data-gtl-find-active", "").is_err() {
        let _ = style.remove_property("outline");
        let _ = style.remove_property("outline-offset");
        return false;
    }
    element.scroll_into_view_with_bool(true);
    true
}

#[cfg(feature = "desktop")]
pub(crate) fn clear_diff_search_match() {
    let Some(element) = document()
        .and_then(|document| document.query_selector("[data-gtl-find-active]").ok())
        .flatten()
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let style = element.style();
    let _ = style.remove_property("outline");
    let _ = style.remove_property("outline-offset");
    let _ = element.remove_attribute("data-gtl-find-active");
}

#[cfg(any(feature = "artifact", feature = "desktop"))]
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

#[cfg(any(feature = "artifact", feature = "desktop"))]
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
