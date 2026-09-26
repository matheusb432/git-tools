use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout};

use super::super::scroll_area::DiffRowsScrollArea;
use crate::{
    entities::diffs::{
        ClientDiffFile, ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffRowsStoreExt,
    },
    shared::{
        i18n::{t, use_language},
        ui::{Button, ButtonSize, ButtonVariant},
    },
    views::diffs::{SplitDiffRowBatch, UnifiedDiffRowBatch},
};

#[component]
pub(super) fn DiffFileBody(
    file: ReadStore<ClientDiffFile>,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    file_index: usize,
    onretry: EventHandler<()>,
    retry_allowed: bool,
) -> Element {
    let row_container_id = format!("viewer-diff-{file_index}");
    let rows = file.rows();
    let unified_batches = rows.unified();
    let split_batches = rows.split();

    rsx! {
        DiffRowsScrollArea {
            id: row_container_id,
            layout,
            density,
            line_number_digits: file.line_number_digits().cloned(),
            footer: rsx! {
                DiffFileLoadState { state: file.state(), retry_allowed, onretry }
            },
            if layout == ViewerDiffLayout::Unified {
                for batch_index in 0..unified_batches.len() {
                    UnifiedDiffRowBatch {
                        key: "{file_index}:{batch_index}",
                        file,
                        batch_index,
                    }
                }
            } else {
                for batch_index in 0..split_batches.len() {
                    SplitDiffRowBatch {
                        key: "{file_index}:{batch_index}",
                        file,
                        batch_index,
                    }
                }
            }
        }
    }
}

#[component]
pub(in crate::views::diffs::client_diff_document) fn DiffFileLoadState(
    state: ReadSignal<ClientDiffFileState>,
    retry_allowed: bool,
    onretry: EventHandler<()>,
) -> Element {
    match &*state.read() {
        ClientDiffFileState::Loading => rsx! {},
        ClientDiffFileState::Complete => rsx! {},
        ClientDiffFileState::Error(error) => {
            let message = error.message(use_language());
            let retryable = error.retryable() && retry_allowed;
            rsx! {
                DiffFileLoadError { message, retryable, onretry }
            }
        }
    }
}

#[component]
fn DiffFileLoadError(message: String, retryable: bool, onretry: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "m-2 flex items-center justify-between gap-3 rounded-sm border border-del-line bg-del-bg px-3 py-2 text-del",
            role: "alert",
            span { "{message}" }
            if retryable {
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Destructive,
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        onretry.call(());
                    },
                    {t!(use_language(), "action-retry")}
                }
            }
        }
    }
}
