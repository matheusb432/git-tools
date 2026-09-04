use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout};

#[cfg(feature = "desktop")]
use crate::shared::ui::{Button, ButtonSize, ButtonVariant};
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
    rsx! {
        div { class: "overflow-hidden",
            DiffFileRows {
                file,
                layout,
                density,
                file_index,
                onretry,
                retry_allowed,
                artifact_file_id,
            }
        }
    }
}

#[component]
fn DiffFileRows(
    file: ReadStore<ClientDiffFile>,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    file_index: usize,
    onretry: EventHandler<()>,
    retry_allowed: bool,
    artifact_file_id: Option<String>,
) -> Element {
    let density_label = density.as_str();
    let layout_label = layout.as_str();
    let style = unified_line_number_width_style(layout, file.line_number_digits().cloned());
    let artifact_enhancement = artifact_file_id.is_some();
    let row_container_id = diff_rows_id(file_index, artifact_file_id.as_deref());
    let rows = file.rows();
    let unified_batches = rows.unified();
    let split_batches = rows.split();
    let unified_batch_count = unified_batches.len();
    let split_batch_count = split_batches.len();

    rsx! {
        div {
            id: row_container_id,
            class: "overflow-x-hidden text-sm leading-5",
            style,
            aria_label: "{layout_label} {density_label} diff rows",
            "data-layout": layout_label,
            "data-density": density_label,
            if layout == ViewerDiffLayout::Unified {
                for batch_index in 0..unified_batch_count {
                    UnifiedDiffRowBatch {
                        key: "{file_index}:{batch_index}",
                        file_index,
                        batch_index,
                        artifact_enhancement,
                    }
                }
            } else {
                for batch_index in 0..split_batch_count {
                    SplitDiffRowBatch {
                        key: "{file_index}:{batch_index}",
                        file_index,
                        batch_index,
                        artifact_enhancement,
                    }
                }
            }
            DiffFileLoadState { state: file.state(), retry_allowed, onretry }
        }
    }
}

fn diff_rows_id(file_index: usize, artifact_file_id: Option<&str>) -> String {
    artifact_file_id.map_or_else(
        || format!("viewer-diff-{file_index}"),
        |file_id| format!("{file_id}-rows"),
    )
}

fn unified_line_number_width_style(
    layout: ViewerDiffLayout,
    line_number_digit_width: u32,
) -> Option<String> {
    (layout == ViewerDiffLayout::Unified)
        .then(|| format!("--unified-line-number-width:calc({line_number_digit_width}ch + 8px)"))
}

#[component]
fn DiffFileLoadState(
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
            let message = error.message().to_owned();
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
                    "Retry"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digit_width_sizes_only_the_unified_line_number_gutter() {
        assert_eq!(
            unified_line_number_width_style(ViewerDiffLayout::Unified, 5),
            Some("--unified-line-number-width:calc(5ch + 8px)".to_owned())
        );
        assert_eq!(
            unified_line_number_width_style(ViewerDiffLayout::Split, 5),
            None
        );
    }

    #[test]
    fn artifact_row_container_uses_the_qualified_file_identity() {
        assert_eq!(diff_rows_id(3, None), "viewer-diff-3");
        assert_eq!(
            diff_rows_id(3, Some("artifact-view-7-file-0")),
            "artifact-view-7-file-0-rows"
        );
    }
}
