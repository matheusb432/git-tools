use std::rc::Rc;

use dioxus::prelude::*;

use super::LoadingSpinner;

#[derive(Clone)]
pub(crate) struct InlineTextSubmission {
    pub value: String,
    pub complete: Rc<dyn Fn(Result<(), String>)>,
}

#[component]
pub(crate) fn InlineTextEditor(
    initial_value: String,
    label: String,
    placeholder: String,
    width_text: String,
    onsubmit: EventHandler<InlineTextSubmission>,
    onfinish: EventHandler<bool>,
) -> Element {
    let mut draft = use_signal(|| initial_value);
    let mut focus_after = use_signal(|| false);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let complete = Rc::new(move |result: Result<(), String>| {
        let mut pending = pending;
        let mut error = error;
        let Ok(mut pending) = pending.try_write() else {
            return;
        };
        *pending = false;
        drop(pending);
        match result {
            Ok(()) => onfinish.call(focus_after()),
            Err(message) => error.set(Some(message)),
        }
    });
    let submit = use_callback(move |focus: bool| {
        if pending() {
            return;
        }
        focus_after.set(focus);
        let value = draft().trim().to_owned();
        if value.is_empty() {
            onfinish.call(focus_after());
            return;
        }
        pending.set(true);
        error.set(None);
        onsubmit.call(InlineTextSubmission {
            value,
            complete: complete.clone(),
        });
    });
    rsx! {
        div {
            class: "inline-text-editor",
            "data-pending": pending().to_string(),
            onpointerdown: move |event| event.stop_propagation(),
            onmouseup: move |event| event.stop_propagation(),
            oncontextmenu: move |event| event.stop_propagation(),
            onclick: move |event| event.stop_propagation(),
            onkeydown: move |event: KeyboardEvent| {
                event.stop_propagation();
                if event.is_composing() || pending() {
                    return;
                }
                match event.key() {
                    Key::Enter => {
                        event.prevent_default();
                        submit.call(true);
                    }
                    Key::Escape => {
                        event.prevent_default();
                        onfinish.call(true);
                    }
                    _ => {}
                }
            },
            span { class: "inline-text-editor-width", aria_hidden: "true", "{width_text}" }
            input {
                class: "inline-text-editor-input",
                r#type: "text",
                aria_label: label,
                aria_invalid: error().is_some().to_string(),
                aria_busy: pending().to_string(),
                title: error().unwrap_or_else(|| "Enter to save, Escape to cancel".to_owned()),
                placeholder,
                value: draft(),
                maxlength: 200,
                readonly: pending(),
                onmounted: move |event| async move {
                    let _ = event.data().set_focus(true).await;
                },
                oninput: move |event| {
                    draft.set(event.value());
                    error.set(None);
                },
                onblur: move |_| {
                    if error().is_none() {
                        submit.call(false);
                    }
                },
            }
            if pending() {
                span {
                    class: "inline-text-editor-status",
                    role: "status",
                    aria_label: "Saving name",
                    LoadingSpinner {}
                }
            }
            if let Some(message) = error() {
                span { class: "inline-text-editor-error", role: "alert", "{message}" }
            }
        }
    }
}
