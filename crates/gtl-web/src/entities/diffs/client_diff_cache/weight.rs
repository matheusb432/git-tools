use gtl_wire::viewer::{ViewerCodeLine, ViewerSplitRow, ViewerUnifiedRow};

use super::super::client_diff::ClientDiffWorkspace;

pub(super) fn workspace_bytes(workspace: &ClientDiffWorkspace) -> usize {
    let mut bytes = workspace.files.capacity()
        * std::mem::size_of::<super::super::client_diff::ClientDiffFile>();
    for file in &workspace.files {
        bytes += file.rows.unified.capacity() * std::mem::size_of::<Vec<ViewerUnifiedRow>>();
        bytes += file.rows.split.capacity() * std::mem::size_of::<Vec<ViewerSplitRow>>();
        for batch in &file.rows.unified {
            bytes += batch.capacity() * std::mem::size_of::<ViewerUnifiedRow>();
            bytes += batch.iter().map(unified_bytes).sum::<usize>();
        }
        for batch in &file.rows.split {
            bytes += batch.capacity() * std::mem::size_of::<ViewerSplitRow>();
            bytes += batch.iter().map(split_bytes).sum::<usize>();
        }
    }
    bytes
}

fn code_bytes(code: &ViewerCodeLine) -> usize {
    code.text.capacity()
        + code.spans.capacity() * std::mem::size_of::<gtl_wire::viewer::ViewerCodeSpan>()
}

fn unified_bytes(row: &ViewerUnifiedRow) -> usize {
    match row {
        ViewerUnifiedRow::Meta(text) | ViewerUnifiedRow::Hunk(text) => text.capacity(),
        ViewerUnifiedRow::Context(row)
        | ViewerUnifiedRow::Added(row)
        | ViewerUnifiedRow::Removed(row) => code_bytes(&row.code),
    }
}

fn split_bytes(row: &ViewerSplitRow) -> usize {
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
