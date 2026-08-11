use std::time::Duration;

use dioxus::prelude::*;

use crate::{
    entities::diffs::ClientDiffFile,
    shared::{
        browser,
        ui::{Button, ButtonSize, ButtonVariant},
    },
};

#[component]
pub(super) fn DiffFileActions(
    file: ClientDiffFile,
    copy_context_enabled: bool,
    onopen: Option<EventHandler<String>>,
) -> Element {
    rsx! {
        span { class: "flex flex-none items-center gap-2 mobile:basis-full mobile:justify-end",
            DiffCopyActions { file: file.clone(), copy_context_enabled }
            DiffLineStats { added: file.summary.added, removed: file.summary.removed }
            if let Some(onopen) = onopen.filter(|_| file.summary.can_open_in_editor) {
                OpenInEditorAction { path: file.summary.path, onopen }
            }
        }
    }
}

#[component]
fn DiffCopyActions(file: ClientDiffFile, copy_context_enabled: bool) -> Element {
    rsx! {
        span { class: "flex flex-none gap-[5px] print:hidden!",
            DiffCopyAction { label: "path", payload: file.summary.path.clone() }
            DiffCopyAction { label: "abs", payload: file.summary.absolute_path.clone() }
            DiffCodeCopyAction { file, include_context: copy_context_enabled }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyState {
    Idle,
    Copied,
    Failed,
}

#[component]
fn DiffCopyAction(label: &'static str, payload: String) -> Element {
    let state = use_signal(|| CopyState::Idle);
    rsx! {
        CopyButton {
            label,
            state: state(),
            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                let payload = payload.clone();
                spawn(async move {
                    update_copy_state(state, &payload).await;
                });
            },
        }
    }
}

#[component]
fn DiffCodeCopyAction(file: ClientDiffFile, include_context: bool) -> Element {
    let state = use_signal(|| CopyState::Idle);
    rsx! {
        CopyButton {
            label: "code",
            state: state(),
            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                let payload = file.copy_code(include_context);
                spawn(async move {
                    update_copy_state(state, &payload).await;
                });
            },
        }
    }
}

#[component]
fn CopyButton(label: &'static str, state: CopyState, onclick: EventHandler<MouseEvent>) -> Element {
    let display = match state {
        CopyState::Idle => label,
        CopyState::Copied => "copied",
        CopyState::Failed => "failed",
    };
    let variant = match state {
        CopyState::Idle => ButtonVariant::Secondary,
        CopyState::Copied => ButtonVariant::Success,
        CopyState::Failed => ButtonVariant::Failure,
    };
    rsx! {
        Button { size: ButtonSize::Micro, variant, onclick, "{display}" }
    }
}

async fn update_copy_state(mut state: Signal<CopyState>, payload: &str) {
    state.set(if browser::copy_text(payload).await {
        CopyState::Copied
    } else {
        CopyState::Failed
    });
    dioxus_sdk_time::sleep(Duration::from_millis(1_200)).await;
    state.set(CopyState::Idle);
}

#[component]
fn DiffLineStats(added: u32, removed: u32) -> Element {
    rsx! {
        span { class: "flex-none text-[12.5px]",
            span { class: "text-add", "data-lines-added": added, "+{added}" }
            " "
            span { class: "text-del", "data-lines-removed": removed, "−{removed}" }
        }
    }
}

#[component]
fn OpenInEditorAction(path: String, onopen: EventHandler<String>) -> Element {
    rsx! {
        Button {
            class: "size-[22px] p-0",
            size: ButtonSize::Content,
            variant: ButtonVariant::Ghost,
            aria_label: "Open in IDE",
            title: "Open in IDE",
            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                onopen.call(path.clone());
            },
            OpenInEditorIcon {}
        }
    }
}

#[component]
fn OpenInEditorIcon() -> Element {
    rsx! {
        span { aria_hidden: "true",
            svg {
                view_box: "0 0 16 16",
                width: "16",
                height: "16",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                path { d: "M9 2.5h4.5V7" }
                path { d: "m13.5 2.5-7 7" }
                path { d: "M7 4H3.5A1.5 1.5 0 0 0 2 5.5v7A1.5 1.5 0 0 0 3.5 14h7a1.5 1.5 0 0 0 1.5-1.5V9" }
            }
        }
    }
}
