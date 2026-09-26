#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use dioxus::prelude::spawn;
use gtl_models::settings::ViewerLanguage;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{HtmlDetailsElement, HtmlDocument, HtmlElement, HtmlTextAreaElement};

#[cfg(target_arch = "wasm32")]
const VIEWER_LANGUAGE_STORAGE_KEY: &str = "gtl.viewer.language";

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
struct WindowKeydownListener {
    window: web_sys::Window,
    callback: Rc<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::KeyboardEvent)>>,
}

pub(crate) fn apply_theme(theme: &'static str) {
    let Some(root) = document().and_then(|document| document.document_element()) else {
        return;
    };
    let _ = root.set_attribute("data-theme", theme);
}

/// Names the language of the document's copy for assistive technology and spellcheck.
pub(crate) fn apply_document_language(language: ViewerLanguage) {
    let Some(root) = document().and_then(|document| document.document_element()) else {
        return;
    };
    let _ = root.set_attribute("lang", language.as_str());
}

/// Returns the language the viewer last displayed, so a restart renders it
/// before the server's settings arrive.
#[cfg(target_arch = "wasm32")]
pub(crate) fn stored_viewer_language() -> Option<ViewerLanguage> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(VIEWER_LANGUAGE_STORAGE_KEY)
        .ok()??
        .parse()
        .ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const fn stored_viewer_language() -> Option<ViewerLanguage> {
    None
}

/// Remembers the displayed language for [`stored_viewer_language`].
#[cfg(target_arch = "wasm32")]
pub(crate) fn store_viewer_language(language: ViewerLanguage) {
    let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    else {
        return;
    };
    let _ = storage.set_item(VIEWER_LANGUAGE_STORAGE_KEY, language.as_str());
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const fn store_viewer_language(_language: ViewerLanguage) {}

pub(crate) fn apply_reduced_motion(reduce_motion: bool) {
    let Some(root) = document().and_then(|document| document.document_element()) else {
        return;
    };
    let _ = root.set_attribute(
        "data-reduce-motion",
        if reduce_motion { "true" } else { "false" },
    );
}

pub(crate) fn focus_element(id: String) {
    let focus = async move {
        dioxus_sdk_time::sleep(Duration::ZERO).await;
        let Some(element) = document()
            .and_then(|document| document.get_element_by_id(&id))
            .and_then(|element| element.dyn_into::<HtmlElement>().ok())
        else {
            return;
        };
        let _ = element.focus();
    };
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(focus);
    #[cfg(not(target_arch = "wasm32"))]
    spawn(focus);
}

pub(crate) fn element_has_visible_focus(id: &str) -> bool {
    document()
        .and_then(|document| document.get_element_by_id(id))
        .is_some_and(|element| element.matches(":focus-visible").unwrap_or(false))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn use_window_keydown(handler: impl FnMut(web_sys::KeyboardEvent) + 'static) {
    let _listener = dioxus::dioxus_core::use_hook_with_cleanup(
        || {
            let window = web_sys::window()?;
            let callback = Rc::new(wasm_bindgen::closure::Closure::wrap(
                Box::new(handler) as Box<dyn FnMut(web_sys::KeyboardEvent)>
            ));
            window
                .add_event_listener_with_callback_and_bool(
                    "keydown",
                    callback.as_ref().as_ref().unchecked_ref(),
                    true,
                )
                .ok()?;
            Some(WindowKeydownListener { window, callback })
        },
        |listener| {
            let Some(listener) = listener else {
                return;
            };
            let _ = listener
                .window
                .remove_event_listener_with_callback_and_bool(
                    "keydown",
                    listener.callback.as_ref().as_ref().unchecked_ref(),
                    true,
                );
        },
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn use_window_keydown(_handler: impl FnMut(web_sys::KeyboardEvent) + 'static) {}

pub(crate) fn popover_is_open(id: &str) -> bool {
    document()
        .and_then(|document| document.get_element_by_id(id))
        .is_some_and(|element| element.matches(":popover-open").unwrap_or(false))
}

pub(crate) fn hide_popover(id: &str) {
    let Some(element) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let _ = element.hide_popover();
}

pub(crate) fn show_hover_popover(id: &str) {
    let Some(document) = document() else {
        return;
    };
    let tooltip = document.get_element_by_id(id);
    let menu_open = document
        .query_selector_all("[popover]:popover-open:not([role='tooltip'])")
        .is_ok_and(|popovers| {
            (0..popovers.length())
                .filter_map(|index| popovers.item(index))
                .any(|popover| !popover.contains(tooltip.as_ref().map(AsRef::as_ref)))
        });
    let dialog_open = document
        .query_selector("dialog[open]")
        .ok()
        .flatten()
        .is_some();
    let inside_dialog = document
        .get_element_by_id(id)
        .and_then(|element| element.closest("dialog[open]").ok().flatten())
        .is_some();
    // An auto tooltip would dismiss an open editor or menu when its delay expires.
    if !menu_open && (!dialog_open || inside_dialog) {
        show_popover(id);
    }
}

pub(crate) fn show_popover(id: &str) {
    let Some(element) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let _ = element.show_popover();
}

pub(crate) fn scroll_to_file(id: &str) {
    let Some(details) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlDetailsElement>().ok())
    else {
        return;
    };
    if !details.open() {
        details.set_open(true);
    }
    details.scroll_into_view_with_bool(true);
}

pub(crate) fn scroll_option_into_view(id: &str) {
    let Some(option) = document()
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let Some(list) = option.parent_element() else {
        return;
    };
    let top = option.offset_top();
    let bottom = top + option.offset_height();
    if top < list.scroll_top() {
        list.set_scroll_top(top);
    } else if bottom > list.scroll_top() + list.client_height() {
        list.set_scroll_top(bottom - list.client_height());
    }
}

pub(crate) fn scroll_diff_document_to_start() {
    let Some(diff_document) = document()
        .and_then(|document| document.query_selector("[data-gtl-diff-document]").ok())
        .flatten()
    else {
        return;
    };
    diff_document.set_scroll_top(0);
}

pub(crate) fn highlight_diff_search_match(file_index: usize, row_index: usize) -> bool {
    let Some(element) = document()
        .and_then(|document| document.get_element_by_id(&format!("viewer-diff-{file_index}")))
        .and_then(|container| {
            container
                .query_selector(&format!("[data-row-index='{row_index}']"))
                .ok()
                .flatten()
        })
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return false;
    };
    if element.has_attribute("data-gtl-find-active") {
        return true;
    }
    clear_diff_search_match();
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
    true
}

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

/// Returns the current instant from the `WebView` clock.
#[cfg(target_arch = "wasm32")]
pub(crate) fn current_timestamp() -> jiff::Timestamp {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Date.now() returns whole milliseconds well inside i64"
    )]
    let milliseconds = web_sys::js_sys::Date::now() as i64;
    jiff::Timestamp::from_millisecond(milliseconds).unwrap_or(jiff::Timestamp::UNIX_EPOCH)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn current_timestamp() -> jiff::Timestamp {
    jiff::Timestamp::now()
}

/// Returns the `WebView` local UTC offset at `instant`, which follows daylight saving time.
#[cfg(target_arch = "wasm32")]
pub(crate) fn local_offset_at(instant: jiff::Timestamp) -> jiff::tz::Offset {
    #[expect(
        clippy::cast_precision_loss,
        reason = "supported timestamps stay far below 2^53 milliseconds"
    )]
    let date = web_sys::js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(
        instant.as_millisecond() as f64,
    ));
    // `getTimezoneOffset()` returns UTC minus local time in whole minutes.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "offsets are whole minutes within one day"
    )]
    let offset_minutes = -(date.get_timezone_offset() as i32);
    jiff::tz::Offset::from_seconds(offset_minutes * 60).unwrap_or(jiff::tz::Offset::UTC)
}

/// Native builds only run tests, which display UTC.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const fn local_offset_at(_instant: jiff::Timestamp) -> jiff::tz::Offset {
    jiff::tz::Offset::UTC
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn use_document_visible() -> dioxus::prelude::ReadSignal<bool> {
    use dioxus::prelude::*;
    let mut visible = use_signal(|| document().is_some_and(|document| !document.hidden()));
    let _listener = dioxus::dioxus_core::use_hook_with_cleanup(
        || {
            let document = document()?;
            let observed = document.clone();
            let callback = Rc::new(wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                visible.set(!observed.hidden());
            })
                as Box<dyn FnMut()>));
            document
                .add_event_listener_with_callback(
                    "visibilitychange",
                    callback.as_ref().as_ref().unchecked_ref(),
                )
                .ok()?;
            Some((document, callback))
        },
        |listener| {
            if let Some((document, callback)) = listener {
                let _ = document.remove_event_listener_with_callback(
                    "visibilitychange",
                    callback.as_ref().as_ref().unchecked_ref(),
                );
            }
        },
    );
    visible.into()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn use_document_visible() -> dioxus::prelude::ReadSignal<bool> {
    dioxus::prelude::use_signal(|| true).into()
}

pub(crate) fn workspace_is_wide() -> bool {
    web_sys::window()
        .and_then(|window| window.inner_width().ok())
        .and_then(|width| width.as_f64())
        .is_some_and(|width| width >= 1025.0)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn use_window_resize(handler: impl FnMut() + 'static) {
    let _listener = dioxus::dioxus_core::use_hook_with_cleanup(
        || {
            let window = web_sys::window()?;
            let callback = Rc::new(wasm_bindgen::closure::Closure::<dyn FnMut()>::new(handler));
            window
                .add_event_listener_with_callback(
                    "resize",
                    callback.as_ref().as_ref().unchecked_ref(),
                )
                .ok()?;
            Some((window, callback))
        },
        |listener| {
            if let Some((window, callback)) = listener {
                let _ = window.remove_event_listener_with_callback(
                    "resize",
                    callback.as_ref().as_ref().unchecked_ref(),
                );
            }
        },
    );
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn use_window_resize(_handler: impl FnMut() + 'static) {}

pub(crate) fn scroll_element_to_start(id: &str) {
    if let Some(element) = document().and_then(|document| document.get_element_by_id(id)) {
        element.set_scroll_top(0);
    }
}
