use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::{ViewerDiffFileId, ViewerFileSummary};
use lucide_dioxus::ExternalLink;

use crate::shared::{
    browser,
    i18n::{t, use_language},
    ui::{
        Button, ButtonSize, ButtonVariant, IconPopover, MENU_ACTION_HOST_CLASSES,
        MenuActionContent, popover::PopoverPlacement,
    },
};

#[component]
pub(super) fn DiffFileActions(
    summary: ReadSignal<ViewerFileSummary>,
    copy_popover_id: String,
    onopen: Option<EventHandler<ViewerDiffFileId>>,
) -> Element {
    let (file_id, can_open_in_editor) =
        summary.with(|summary| (summary.id.clone(), summary.can_open_in_editor));
    rsx! {
        span {
            class: "diff-file-actions",
            "data-tour": super::super::tour::READING_ACTIONS.value(),
            if let Some(review) = summary.read().review.clone() {
                crate::views::diffs::file_review::FileReviewAction { review }
            }
            DiffPathCopyMenu { summary, popover_id: copy_popover_id }
            if let Some(onopen) = onopen.filter(|_| can_open_in_editor) {
                OpenInTextEditorAction { file_id, onopen }
            }
        }
    }
}

#[component]
fn DiffPathCopyMenu(summary: ReadSignal<ViewerFileSummary>, popover_id: String) -> Element {
    let (relative_path, absolute_path) = summary.with(|summary| {
        (
            summary.path.to_string_lossy().into_owned(),
            summary
                .absolute_path
                .as_path()
                .to_string_lossy()
                .into_owned(),
        )
    });

    rsx! {
        span {
            class: "diff-file-copy-menu print:hidden!",
            onclick: move |event: MouseEvent| event.stop_propagation(),
            IconPopover {
                id: popover_id.clone(),
                aria_label: t!(use_language(), "copy-file-path"),
                placement: PopoverPlacement::TriggerEnd,
                icon: rsx! {
                    CopyMark {}
                },
                span { class: "grid gap-0.5 p-1.5",
                    DiffPathCopyAction {
                        kind: DiffPathCopyKind::Relative,
                        payload: relative_path,
                        popover_id: popover_id.clone(),
                    }
                    DiffPathCopyAction {
                        kind: DiffPathCopyKind::Absolute,
                        payload: absolute_path,
                        popover_id,
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

#[derive(Clone, Copy)]
struct CopyFeedbackController {
    state: ReadSignal<CopyState>,
    copy: Callback<()>,
}

impl CopyState {
    const fn value(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Copied => "success",
            Self::Failed => "failure",
        }
    }

    fn message(self, language: ViewerLanguage) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Copied => t!(language, "copy-copied"),
            Self::Failed => t!(language, "copy-failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffPathCopyKind {
    Relative,
    Absolute,
}

impl DiffPathCopyKind {
    fn label(self, language: ViewerLanguage) -> String {
        match self {
            Self::Relative => t!(language, "copy-relative-path"),
            Self::Absolute => t!(language, "copy-absolute-path"),
        }
    }

    fn aria_label(self, language: ViewerLanguage) -> String {
        match self {
            Self::Relative => t!(language, "copy-relative-path-action"),
            Self::Absolute => t!(language, "copy-absolute-path-action"),
        }
    }
}

fn use_copy_feedback(payload: String) -> CopyFeedbackController {
    let state = use_signal(|| CopyState::Idle);
    let copy_action = use_action(move || {
        let payload = payload.clone();
        async move {
            update_copy_state(state, &payload).await;
            Ok::<(), std::convert::Infallible>(())
        }
    });
    let copy = use_callback(move |()| {
        let mut copy_action = copy_action;
        copy_action.call();
    });

    CopyFeedbackController {
        state: state.into(),
        copy,
    }
}

#[component]
fn DiffPathCopyAction(kind: DiffPathCopyKind, payload: String, popover_id: String) -> Element {
    let language = use_language();
    let feedback = use_copy_feedback(payload);

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
            aria_label: kind.aria_label(language),

            onclick: move |event: MouseEvent| {
                event.prevent_default();
                event.stop_propagation();
                feedback.copy.call(());
                browser::hide_popover(&popover_id);
            },
            MenuActionContent { icon, label: kind.label(language),
                CopyActionFeedback { state: (feedback.state)() }
            }
        }
    }
}

#[component]
fn CopyActionFeedback(state: CopyState) -> Element {
    rsx! {
        span {
            class: "diff-file-copy-feedback",
            role: "status",
            aria_live: "polite",
            aria_atomic: "true",
            "data-state": state.value(),

            {state.message(use_language())}
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
    let language = use_language();
    rsx! {
        Button {
            class: "diff-file-open-action",
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: t!(language, "diff-open-in-editor"),
            title: t!(language, "diff-open-in-editor"),
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
