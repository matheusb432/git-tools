use gtl_wire::viewer::{ViewerCodeLine, ViewerSplitRow, ViewerUnifiedRow};

use crate::entities::diffs::ClientDiffRows;

pub(super) fn line(
    rows: &ClientDiffRows,
    row: usize,
    old: bool,
) -> Result<Option<(u32, &str)>, ()> {
    let source = if rows.unified.is_empty() {
        let row = rows
            .split
            .get(row / 64)
            .and_then(|rows| rows.get(row % 64))
            .ok_or(())?;
        split_line(row, old)
    } else {
        let row = rows
            .unified
            .get(row / 64)
            .and_then(|rows| rows.get(row % 64))
            .ok_or(())?;
        unified_line(row, old)
    };
    source
        .map(|(number, code)| {
            if code.omitted_character_count.is_some() {
                Err(())
            } else {
                Ok((number, code.text.as_str()))
            }
        })
        .transpose()
}

fn unified_line(row: &ViewerUnifiedRow, old: bool) -> Option<(u32, &ViewerCodeLine)> {
    match row {
        ViewerUnifiedRow::Meta(_) | ViewerUnifiedRow::Hunk(_) => None,
        ViewerUnifiedRow::Context(source)
        | ViewerUnifiedRow::Added(source)
        | ViewerUnifiedRow::Removed(source) => (if old {
            source.old_line_number
        } else {
            source.new_line_number
        })
        .map(|number| (number, &source.code)),
    }
}

fn split_line(row: &ViewerSplitRow, old: bool) -> Option<(u32, &ViewerCodeLine)> {
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
            code,
        )),
        ViewerSplitRow::Pair { old: before, new } => (if old { before } else { new })
            .as_ref()
            .map(|cell| (cell.line_number, &cell.code)),
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerSplitCell;

    use super::*;
    use crate::test_support::{code_line, unified_source_row};

    #[test]
    fn truncated_source_requires_a_fetch_for_either_side_and_layout() {
        for (omitted, old, expected) in [
            (None, false, Ok(Some((9, "preview")))),
            (None, true, Ok(Some((7, "preview")))),
            (Some(92_718), false, Err(())),
            (Some(92_718), true, Err(())),
        ] {
            let unified = ClientDiffRows {
                unified: vec![vec![ViewerUnifiedRow::Context(unified_source_row(
                    "preview",
                    Some(7),
                    Some(9),
                    omitted,
                ))]],
                split: vec![],
            };
            let split = ClientDiffRows {
                split: vec![vec![
                    ViewerSplitRow::Pair {
                        old: Some(ViewerSplitCell {
                            line_number: 7,
                            code: code_line("preview", omitted),
                        }),
                        new: Some(ViewerSplitCell {
                            line_number: 9,
                            code: code_line("preview", omitted),
                        }),
                    },
                    ViewerSplitRow::Context {
                        old_line_number: 7,
                        new_line_number: 9,
                        code: code_line("preview", omitted),
                    },
                ]],
                unified: vec![],
            };
            assert_eq!(line(&unified, 0, old), expected);
            assert_eq!(line(&split, 0, old), expected);
            assert_eq!(line(&split, 1, old), expected);
        }
    }
}
