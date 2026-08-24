use std::time::Duration;

use dioxus::prelude::*;
use gtl_wire::viewer::ViewerDiffFileId;
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
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    artifact_enhancement: bool,
) -> Element {
    rsx! {
        span { class: "flex flex-none items-center gap-2 mobile:hidden",
            DiffCopyActions {
                file: file.clone(),
                copy_context_enabled,
                artifact_enhancement,
            }
            if let Some(onopen) = onopen.filter(|_| file.summary.can_open_in_editor) {
                OpenInTextEditorAction { file_id: file.summary.id, onopen }
            }
        }
    }
}

#[component]
fn DiffCopyActions(
    file: ClientDiffFile,
    copy_context_enabled: bool,
    artifact_enhancement: bool,
) -> Element {
    rsx! {
        span { class: "flex flex-none gap-2 print:hidden!",
            DiffCopyAction {
                kind: DiffCopyKind::Path,
                payload: file.summary.path.to_string_lossy().into_owned(),
                artifact_enhancement,
            }
            DiffCopyAction {
                kind: DiffCopyKind::Absolute,
                payload: file.summary.absolute_path.as_path().to_string_lossy().into_owned(),
                artifact_enhancement,
            }
            DiffCodeCopyAction {
                file,
                include_context: copy_context_enabled,
                artifact_enhancement,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyState {
    Idle,
    Copied,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffCopyKind {
    Path,
    Absolute,
    Code,
}

impl DiffCopyKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Absolute => "abs",
            Self::Code => "code",
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Absolute => "absolute",
            Self::Code => "code",
        }
    }
}

#[component]
fn DiffCopyAction(kind: DiffCopyKind, payload: String, artifact_enhancement: bool) -> Element {
    let state = use_signal(|| CopyState::Idle);
    rsx! {
        CopyButton {
            kind,
            state: state(),
            artifact_enhancement,
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
fn DiffCodeCopyAction(
    file: ClientDiffFile,
    include_context: bool,
    artifact_enhancement: bool,
) -> Element {
    let state = use_signal(|| CopyState::Idle);
    rsx! {
        CopyButton {
            kind: DiffCopyKind::Code,
            state: state(),
            artifact_enhancement,
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
fn CopyButton(
    kind: DiffCopyKind,
    state: CopyState,
    artifact_enhancement: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let label = kind.label();
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
    let artifact_copy = artifact_enhancement.then_some(kind.as_str());
    let artifact_idle_label = artifact_enhancement.then_some(label);
    let artifact_idle_classes = artifact_enhancement.then_some(ButtonVariant::Secondary.classes());
    let artifact_success_classes = artifact_enhancement.then_some(ButtonVariant::Success.classes());
    let artifact_failure_classes = artifact_enhancement.then_some(ButtonVariant::Failure.classes());
    rsx! {
        Button {
            size: ButtonSize::Small,
            variant,
            onclick,
            "data-gtl-copy": artifact_copy,
            "data-gtl-idle-label": artifact_idle_label,
            "data-gtl-idle-classes": artifact_idle_classes,
            "data-gtl-success-classes": artifact_success_classes,
            "data-gtl-failure-classes": artifact_failure_classes,
            "{display}"
        }
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
    file_id: ViewerDiffFileId,
    onopen: EventHandler<ViewerDiffFileId>,
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
                onopen.call(file_id.clone());
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
