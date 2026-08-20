use dioxus::prelude::*;
use gtl_parser::LineNumberDigitWidth;
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout};

#[cfg(feature = "desktop")]
use crate::shared::ui::{Button, ButtonSize, ButtonVariant};
use crate::{
    entities::diffs::{ClientDiffFile, ClientDiffFileState, ClientDiffRows},
    views::diffs::{SplitDiffRowBatch, UnifiedDiffRowBatch},
};

#[component]
pub(super) fn DiffFileBody(
    file: ClientDiffFile,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    file_index: usize,
    onretry: EventHandler<()>,
    artifact_file_id: Option<String>,
) -> Element {
    rsx! {
        div { class: "overflow-hidden rounded-b-panel",
            DiffFileRows {
                file,
                layout,
                density,
                file_index,
                onretry,
                artifact_file_id,
            }
        }
    }
}

#[component]
fn DiffFileRows(
    file: ClientDiffFile,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    file_index: usize,
    onretry: EventHandler<()>,
    artifact_file_id: Option<String>,
) -> Element {
    let density_label = density.as_str();
    let layout_label = layout.as_str();
    let style = unified_line_number_width_style(layout, file.line_number_digits);
    let artifact_enhancement = artifact_file_id.is_some();
    let row_container_id = diff_rows_id(file_index, artifact_file_id.as_deref());

    rsx! {
        div {
            id: row_container_id,
            class: "overflow-x-hidden text-sm leading-5",
            style,
            aria_label: "{layout_label} {density_label} diff rows",
            "data-layout": layout_label,
            "data-density": density_label,
            match &file.rows {
                ClientDiffRows::Unified(batches) => rsx! {
                    for (batch_index, batch) in batches.iter().enumerate() {
                        UnifiedDiffRowBatch { key: "{batch_index}", rows: batch.clone(), artifact_enhancement }
                    }
                },
                ClientDiffRows::Split(batches) => rsx! {
                    for (batch_index, batch) in batches.iter().enumerate() {
                        SplitDiffRowBatch { key: "{batch_index}", rows: batch.clone(), artifact_enhancement }
                    }
                },
            }
            DiffFileLoadState { state: file.state, onretry }
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
    line_number_digit_width: LineNumberDigitWidth,
) -> Option<String> {
    (layout == ViewerDiffLayout::Unified)
        .then(|| format!("--unified-line-number-width:calc({line_number_digit_width}ch + 8px)"))
}

#[component]
fn DiffFileLoadState(state: ClientDiffFileState, onretry: EventHandler<()>) -> Element {
    match state {
        #[cfg(feature = "desktop")]
        ClientDiffFileState::Loading => rsx! {
            DiffFileLoading {}
        },
        ClientDiffFileState::Complete => rsx! {},
        #[cfg(feature = "desktop")]
        ClientDiffFileState::Error(error) => {
            rsx! {
                DiffFileLoadError { message: error.message(), onretry }
            }
        }
    }
}

#[cfg(feature = "desktop")]
#[component]
fn DiffFileLoading() -> Element {
    rsx! {
        div { class: "min-h-[22px] px-3 text-ink-3", role: "status", "Loading…" }
    }
}

#[cfg(feature = "desktop")]
#[component]
fn DiffFileLoadError(message: &'static str, onretry: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "m-2 flex items-center justify-between gap-3 rounded-sm border border-del-line bg-del-bg px-3 py-2 text-del",
            role: "alert",
            span { "{message}" }
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

#[cfg(test)]
mod tests {
    use gtl_parser::DiffParser;

    use super::*;

    #[test]
    fn digit_width_sizes_only_the_unified_line_number_gutter() {
        let width = DiffParser::new()
            .parse(&["@@ -9999 +10000 @@".to_owned(), " keep".to_owned()])
            .line_number_digits();

        assert_eq!(
            unified_line_number_width_style(ViewerDiffLayout::Unified, width),
            Some("--unified-line-number-width:calc(5ch + 8px)".to_owned())
        );
        assert_eq!(
            unified_line_number_width_style(ViewerDiffLayout::Split, width),
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
