use gtl_wire::viewer::{ViewerCodeLine, ViewerSplitRow, ViewerUnifiedRow};

pub(in crate::entities::diffs) fn row_window_bytes(rows: &gtl_wire::viewer::ViewerRows) -> usize {
    match rows {
        gtl_wire::viewer::ViewerRows::Unified(rows) => {
            rows.capacity() * std::mem::size_of::<ViewerUnifiedRow>()
                + rows.iter().map(unified_row_bytes).sum::<usize>()
        }
        gtl_wire::viewer::ViewerRows::Split(rows) => {
            rows.capacity() * std::mem::size_of::<ViewerSplitRow>()
                + rows.iter().map(split_row_bytes).sum::<usize>()
        }
    }
}

fn code_bytes(code: &ViewerCodeLine) -> usize {
    code.text.capacity()
        + code.spans.capacity() * std::mem::size_of::<gtl_wire::viewer::ViewerCodeSpan>()
}

pub(in crate::entities::diffs) fn unified_row_bytes(row: &ViewerUnifiedRow) -> usize {
    match row {
        ViewerUnifiedRow::Meta(text) | ViewerUnifiedRow::Hunk(text) => text.capacity(),
        ViewerUnifiedRow::Context(row)
        | ViewerUnifiedRow::Added(row)
        | ViewerUnifiedRow::Removed(row) => code_bytes(&row.code),
    }
}

pub(in crate::entities::diffs) fn split_row_bytes(row: &ViewerSplitRow) -> usize {
    match row {
        ViewerSplitRow::Meta(text) | ViewerSplitRow::Hunk(text) => text.capacity(),
        ViewerSplitRow::Context { code, .. } => code_bytes(code),
        ViewerSplitRow::Pair { old, new } => old
            .iter()
            .chain(new)
            .map(|cell| code_bytes(&cell.code))
            .sum(),
    }
}
