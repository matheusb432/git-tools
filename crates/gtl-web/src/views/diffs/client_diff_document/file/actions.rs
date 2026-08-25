use std::time::Duration;

use dioxus::prelude::*;
use gtl_wire::viewer::ViewerDiffFileId;
use lucide_dioxus::ExternalLink;

use crate::{
    entities::diffs::ClientDiffFile,
    shared::{
        browser,
        ui::{
            Button, ButtonSize, ButtonVariant, IconPopover, IconPopoverPlacement,
            MENU_ACTION_HOST_CLASSES, MenuActionContent,
        },
    },
};

#[component]
pub(super) fn DiffFileActions(
    file: ClientDiffFile,
    copy_popover_id: String,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
    artifact_enhancement: bool,
) -> Element {
    rsx! {
        span { class: "flex flex-none items-center gap-1 mobile:hidden",
            DiffPathCopyMenu {
                file: file.clone(),
                popover_id: copy_popover_id,
                artifact_enhancement,
            }
            if let Some(onopen) = onopen.filter(|_| file.summary.can_open_in_editor) {
                OpenInTextEditorAction { file_id: file.summary.id, onopen }
            }
        }
    }
}

#[component]
fn DiffPathCopyMenu(
    file: ClientDiffFile,
    popover_id: String,
    artifact_enhancement: bool,
) -> Element {
    let relative_path = file.summary.path.to_string_lossy().into_owned();
    let absolute_path = file
        .summary
        .absolute_path
        .as_path()
        .to_string_lossy()
        .into_owned();

    rsx! {
        span {
            class: "flex flex-none print:hidden!",
            onclick: move |event: MouseEvent| event.stop_propagation(),
            IconPopover {
                id: popover_id,
                aria_label: "Copy file path",
                placement: IconPopoverPlacement::TriggerEnd,
                icon: rsx! {
                    CopyMark {}
                },
                span { class: "grid gap-0.5 p-1.5",
                    DiffPathCopyAction {
                        kind: DiffPathCopyKind::Relative,
                        payload: relative_path,
                        artifact_enhancement,
                    }
                    DiffPathCopyAction {
                        kind: DiffPathCopyKind::Absolute,
                        payload: absolute_path,
                        artifact_enhancement,
                    }
                }
            }
        }
    }
}

#[component]
fn CopyMark() -> Element {
    rsx! {
        span { class: "relative block size-4",
            span { class: "absolute top-0 right-0 size-2.5 rounded-xs border border-current" }
            span { class: "absolute bottom-0 left-0 size-2.5 rounded-xs border border-current" }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyState {
    Idle,
    Copied,
    Failed,
}

impl CopyState {
    const fn value(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Copied => "success",
            Self::Failed => "failure",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::Idle => "",
            Self::Copied => "Copied",
            Self::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffPathCopyKind {
    Relative,
    Absolute,
}

impl DiffPathCopyKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Relative => "Relative path",
            Self::Absolute => "Absolute path",
        }
    }

    const fn aria_label(self) -> &'static str {
        match self {
            Self::Relative => "Copy relative path",
            Self::Absolute => "Copy absolute path",
        }
    }

    const fn artifact_value(self) -> &'static str {
        match self {
            Self::Relative => "path",
            Self::Absolute => "absolute",
        }
    }
}

#[component]
fn DiffPathCopyAction(
    kind: DiffPathCopyKind,
    payload: String,
    artifact_enhancement: bool,
) -> Element {
    let state = use_signal(|| CopyState::Idle);
    let copied_payload = payload.clone();
    let artifact_copy = artifact_enhancement.then_some(kind.artifact_value());
    let icon = match kind {
        DiffPathCopyKind::Relative => rsx! {
            span { class: "text-xs font-bold tracking-tight", "./" }
        },
        DiffPathCopyKind::Absolute => rsx! {
            span { class: "text-sm font-bold", "/" }
        },
    };

    rsx! {
        button {
            class: MENU_ACTION_HOST_CLASSES,
            r#type: "button",
            aria_label: kind.aria_label(),
            "data-gtl-copy": artifact_copy,
            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                let payload = copied_payload.clone();
                spawn(async move {
                    update_copy_state(state, &payload).await;
                });
            },
            MenuActionContent { icon, label: kind.label(),
                CopyActionFeedback { state: state(), artifact_enhancement }
            }
        }
    }
}

#[component]
fn CopyActionFeedback(state: CopyState, artifact_enhancement: bool) -> Element {
    let artifact_feedback = artifact_enhancement.then_some("");

    rsx! {
        span {
            class: "min-w-12 flex-none text-right text-[0.6875rem] font-semibold text-ink-3 data-[state=success]:text-add data-[state=failure]:text-del",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            "data-state": state.value(),
            "data-gtl-copy-feedback": artifact_feedback,
            {state.message()}
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
        Button {
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: "Open in text editor",
            title: "Open in text editor",
            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                onopen.call(file_id.clone());
            },
            span { aria_hidden: "true",
                ExternalLink { size: 16, stroke_width: 2 }
            }
        }
    }
}
