//! Native row mounting boundary for the workspace benchmarks.

use dioxus::prelude::*;
use gtl_wire::viewer::{ViewerFileSummary, ViewerRows};

use crate::{
    entities::diffs::{ClientDiffFile, ClientDiffFileState, ClientDiffRows},
    views::diffs::{SplitDiffRowBatch, UnifiedDiffRowBatch},
};

/// Prepares an unmounted document; call `rebuild_in_place` to measure row mounting.
pub fn row_batches_dom(summary: ViewerFileSummary, rows: ViewerRows) -> VirtualDom {
    let rows = match rows {
        ViewerRows::Unified(rows) => ClientDiffRows {
            unified: batches(rows),
            split: Vec::new(),
        },
        ViewerRows::Split(rows) => ClientDiffRows {
            unified: Vec::new(),
            split: batches(rows),
        },
    };
    VirtualDom::new_with_props(
        RowBatches,
        RowBatchesProps {
            file: ClientDiffFile {
                summary,
                rows,
                line_number_digits: 4,
                state: ClientDiffFileState::Complete,
            },
        },
    )
}

fn batches<Row>(rows: Vec<Row>) -> Vec<Vec<Row>> {
    let mut rows = rows.into_iter();
    std::iter::from_fn(move || {
        let batch = rows.by_ref().take(64).collect::<Vec<_>>();
        (!batch.is_empty()).then_some(batch)
    })
    .collect()
}

#[component]
fn RowBatches(file: ClientDiffFile) -> Element {
    let unified = file.rows.unified.len();
    let split = file.rows.split.len();
    let file = use_store(move || file);
    rsx! {
        for batch_index in 0..unified {
            UnifiedDiffRowBatch { key: "unified-{batch_index}", file, batch_index }
        }
        for batch_index in 0..split {
            SplitDiffRowBatch { key: "split-{batch_index}", file, batch_index }
        }
    }
}
