use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::paths::RepositoryRelativePath;
use lucide_dioxus::ExternalLink;

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
    onopen: Option<EventHandler<RepositoryRelativePath>>,
) -> Element {
    rsx! {
        span { class: "flex flex-none items-center gap-2 mobile:hidden",
            DiffCopyActions { file: file.clone(), copy_context_enabled }
            if let Some(onopen) = onopen.filter(|_| file.summary.can_open_in_editor) {
                OpenInTextEditorAction { path: file.summary.path, onopen }
            }
        }
    }
}

#[component]
fn DiffCopyActions(file: ClientDiffFile, copy_context_enabled: bool) -> Element {
    rsx! {
        span { class: "flex flex-none gap-2 print:hidden!",
            DiffCopyAction {
                label: "path",
                payload: file.summary.path.to_string_lossy().into_owned(),
            }
            DiffCopyAction {
                label: "abs",
                payload: file.summary.absolute_path.as_path().to_string_lossy().into_owned(),
            }
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

// TODO: restyle. looks a bit ugly
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
        Button { size: ButtonSize::Small, variant, onclick, "{display}" }
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
fn OpenInTextEditorAction(
    path: RepositoryRelativePath,
    onopen: EventHandler<RepositoryRelativePath>,
) -> Element {
    rsx! {
        // TODO: make it use icon button primitive (create it)
        Button {
            class: "p-0",
            size: ButtonSize::Content,
            variant: ButtonVariant::Ghost,
            aria_label: "Open in text editor",
            title: "Open in text editor",
            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                onopen.call(path.clone());
            },
            OpenInTextEditorIcon {}
        }
    }
}

#[component]
fn OpenInTextEditorIcon() -> Element {
    rsx! {
        span { aria_hidden: "true",
            ExternalLink { size: 16, stroke_width: 2 }
        }
    }
}
