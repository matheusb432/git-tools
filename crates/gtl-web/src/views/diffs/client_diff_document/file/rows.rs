use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout};

use crate::{
    entities::diffs::{ClientDiffFile, ClientDiffFileState, ClientDiffRows},
    shared::ui::{Button, ButtonSize, ButtonVariant},
    views::diffs::{SplitDiffRowBatch, UnifiedDiffRowBatch},
};

#[component]
pub(super) fn DiffFileBody(
    file: ClientDiffFile,
    layout: ViewerDiffLayout,
    density: ViewerDiffDensity,
    file_index: usize,
    onretry: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "overflow-hidden rounded-b-panel",
            DiffFileRows {
                file,
                layout,
                density,
                file_index,
                onretry,
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
) -> Element {
    let density_label = density.as_str();
    let layout_label = layout.as_str();
    let style = (layout == ViewerDiffLayout::Unified).then(|| {
        format!(
            "--unified-line-number-width:calc({}ch + 8px)",
            file.line_number_digits
        )
    });

    rsx! {
        div {
            id: "viewer-diff-{file_index}",
            class: "overflow-x-hidden text-sm leading-5",
            style,
            aria_label: "{layout_label} {density_label} diff rows",
            "data-layout": layout_label,
            "data-density": density_label,
            match &file.rows {
                ClientDiffRows::Unified(batches) => rsx! {
                    for (batch_index, batch) in batches.iter().enumerate() {
                        UnifiedDiffRowBatch { key: "{batch_index}", rows: batch.clone() }
                    }
                },
                ClientDiffRows::Split(batches) => rsx! {
                    for (batch_index, batch) in batches.iter().enumerate() {
                        SplitDiffRowBatch { key: "{batch_index}", rows: batch.clone() }
                    }
                },
            }
            DiffFileLoadState { state: file.state, onretry }
        }
    }
}

#[component]
fn DiffFileLoadState(state: ClientDiffFileState, onretry: EventHandler<()>) -> Element {
    match state {
        ClientDiffFileState::Loading => rsx! {
            DiffFileLoading {}
        },
        ClientDiffFileState::Complete => rsx! {},
        ClientDiffFileState::Error(error) => {
            rsx! {
                DiffFileLoadError { message: error.message(), onretry }
            }
        }
    }
}

#[component]
fn DiffFileLoading() -> Element {
    rsx! {
        div { class: "min-h-[22px] px-3 text-ink-3", role: "status", "Loading…" }
    }
}

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
