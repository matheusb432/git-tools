use gtl_wire::viewer::{ViewerSplitRow, ViewerUnifiedRow};

use crate::entities::diffs::ClientDiffRows;

pub(super) fn line(
    rows: &ClientDiffRows,
    row: usize,
    old: bool,
) -> Result<Option<(u32, &str)>, ()> {
    if rows.unified.is_empty() {
        let row = rows
            .split
            .get(row / 64)
            .and_then(|rows| rows.get(row % 64))
            .ok_or(())?;
        Ok(split_line(row, old))
    } else {
        let row = rows
            .unified
            .get(row / 64)
            .and_then(|rows| rows.get(row % 64))
            .ok_or(())?;
        Ok(unified_line(row, old))
    }
}

fn unified_line(row: &ViewerUnifiedRow, old: bool) -> Option<(u32, &str)> {
    match row {
        ViewerUnifiedRow::Meta(_) | ViewerUnifiedRow::Hunk(_) => None,
        ViewerUnifiedRow::Context(source)
        | ViewerUnifiedRow::Added(source)
        | ViewerUnifiedRow::Removed(source) => (if old {
            source.old_line_number
        } else {
            source.new_line_number
        })
        .map(|number| (number, source.code.text.as_str())),
    }
}

fn split_line(row: &ViewerSplitRow, old: bool) -> Option<(u32, &str)> {
    match row {
        ViewerSplitRow::Meta(_) | ViewerSplitRow::Hunk(_) => None,
        ViewerSplitRow::Context {
            old_line_number,
            new_line_number,
            code,
        } => Some((
            if old {
                *old_line_number
            } else {
                *new_line_number
            },
            code.text.as_str(),
        )),
        ViewerSplitRow::Pair { old: before, new } => (if old { before } else { new })
            .as_ref()
            .map(|cell| (cell.line_number, cell.code.text.as_str())),
    }
}
