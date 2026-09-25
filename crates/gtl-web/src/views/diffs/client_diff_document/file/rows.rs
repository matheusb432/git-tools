use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout};

use super::super::scroll_area::DiffRowsScrollArea;
#[cfg(feature = "desktop")]
use crate::shared::{
    i18n::{t, use_language},
    ui::{Button, ButtonSize, ButtonVariant},
};
use crate::{
    entities::diffs::{
        ClientDiffFile, ClientDiffFileState, ClientDiffFileStoreExt, ClientDiffRowsStoreExt,
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
    artifact_file_id: Option<String>,
) -> Element {
    let artifact_enhancement = artifact_file_id.is_some();
    let row_container_id = diff_rows_id(file_index, artifact_file_id.as_deref());
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
                        artifact_enhancement,
                    }
                }
            } else {
                for batch_index in 0..split_batches.len() {
                    SplitDiffRowBatch {
                        key: "{file_index}:{batch_index}",
                        file,
                        batch_index,
                        artifact_enhancement,
                    }
                }
            }
        }
    }
}

fn diff_rows_id(file_index: usize, artifact_file_id: Option<&str>) -> String {
    artifact_file_id.map_or_else(
        || format!("viewer-diff-{file_index}"),
        |file_id| format!("{file_id}-rows"),
    )
}

#[component]
pub(in crate::views::diffs::client_diff_document) fn DiffFileLoadState(
    state: ReadSignal<ClientDiffFileState>,
    retry_allowed: bool,
    onretry: EventHandler<()>,
) -> Element {
    match &*state.read() {
        #[cfg(feature = "desktop")]
        ClientDiffFileState::Loading => rsx! {},
        ClientDiffFileState::Complete => rsx! {},
        #[cfg(feature = "desktop")]
        ClientDiffFileState::Error(error) => {
            let message = error.message(use_language());
            let retryable = error.retryable() && retry_allowed;
            rsx! {
                DiffFileLoadError { message, retryable, onretry }
            }
        }
    }
}

#[cfg(feature = "desktop")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_row_container_uses_the_qualified_file_identity() {
        assert_eq!(diff_rows_id(3, None), "viewer-diff-3");
        assert_eq!(
            diff_rows_id(3, Some("artifact-view-7-file-0")),
            "artifact-view-7-file-0-rows"
        );
    }
}
