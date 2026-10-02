use crate::{
    terminal_diff::{self, SnapshotError},
    v1,
    viewer::ViewerCodeLine,
};

pub fn encode_row(row: &terminal_diff::Row) -> Result<v1::TerminalDiffRow, SnapshotError> {
    let syntax = if row.syntax.is_empty() {
        Vec::new()
    } else {
        super::viewer::encode_viewer_code_line(ViewerCodeLine {
            text: row.text.clone(),
            spans: row.syntax.clone(),
            omitted_character_count: None,
        })
        .map_err(|_| SnapshotError::Invalid)?
        .spans
    };
    Ok(v1::TerminalDiffRow {
        kind: match row.kind {
            terminal_diff::RowKind::Meta => v1::TerminalDiffRowKind::Meta,
            terminal_diff::RowKind::Hunk => v1::TerminalDiffRowKind::Hunk,
            terminal_diff::RowKind::Context => v1::TerminalDiffRowKind::Context,
            terminal_diff::RowKind::Added => v1::TerminalDiffRowKind::Added,
            terminal_diff::RowKind::Removed => v1::TerminalDiffRowKind::Removed,
        } as i32,
        text: row.text.clone(),
        old_line_number: row.old_line_number,
        new_line_number: row.new_line_number,
        syntax,
    })
}

fn decode_row(row: v1::TerminalDiffRow) -> Result<terminal_diff::Row, SnapshotError> {
    if row.syntax.len() > terminal_diff::ROW_SYNTAX_SPANS_MAX {
        return Err(SnapshotError::TooLarge);
    }
    let kind = match v1::TerminalDiffRowKind::try_from(row.kind) {
        Ok(v1::TerminalDiffRowKind::Meta) => terminal_diff::RowKind::Meta,
        Ok(v1::TerminalDiffRowKind::Hunk) => terminal_diff::RowKind::Hunk,
        Ok(v1::TerminalDiffRowKind::Context) => terminal_diff::RowKind::Context,
        Ok(v1::TerminalDiffRowKind::Added) => terminal_diff::RowKind::Added,
        Ok(v1::TerminalDiffRowKind::Removed) => terminal_diff::RowKind::Removed,
        _ => return Err(SnapshotError::Invalid),
    };
    let has_syntax = !row.syntax.is_empty();
    let code = super::viewer::decode_viewer_code_line(v1::ViewerCodeLine {
        text: row.text,
        spans: row.syntax,
        omitted_character_count: None,
    })
    .map_err(|_| SnapshotError::Invalid)?;
    Ok(terminal_diff::Row {
        kind,
        text: code.text,
        old_line_number: row.old_line_number,
        new_line_number: row.new_line_number,
        syntax: code
            .spans
            .into_iter()
            .filter(|span| has_syntax && span.byte_start < span.byte_end)
            .collect(),
    })
}

#[derive(Default)]
pub struct SnapshotDecoder {
    header: Option<v1::TerminalDiffHeader>,
    files: Vec<terminal_diff::File>,
    budget: terminal_diff::SnapshotBudget,
    completed: bool,
}

impl SnapshotDecoder {
    pub fn accept(&mut self, response: v1::ReadTerminalDiffResponse) -> Result<(), SnapshotError> {
        use v1::read_terminal_diff_response::Event;
        if self.completed {
            return Err(SnapshotError::Invalid);
        }
        match response.event.ok_or(SnapshotError::Invalid)? {
            Event::Header(header) if self.header.is_none() => {
                self.budget.add_header(&header.title, &header.notes)?;
                self.header = Some(header);
            }
            Event::File(file) if self.header.is_some() => {
                self.budget.add_file(&file.path)?;
                self.files.push(terminal_diff::File {
                    path: file.path,
                    added: file.added,
                    removed: file.removed,
                    compact: Vec::new(),
                    full: Vec::new(),
                });
            }
            Event::Rows(batch) if self.header.is_some() => {
                if batch.rows.is_empty() || batch.rows.len() > terminal_diff::BATCH_ROWS_MAX {
                    return Err(SnapshotError::Invalid);
                }
                let file = self.files.last_mut().ok_or(SnapshotError::Invalid)?;
                if !batch.full_context && !file.full.is_empty() {
                    return Err(SnapshotError::Invalid);
                }
                let target = if batch.full_context {
                    &mut file.full
                } else {
                    &mut file.compact
                };
                for raw in batch.rows {
                    let row = decode_row(raw)?;
                    self.budget.add_row(&row)?;
                    target.push(row);
                }
            }
            Event::Completed(_) if self.header.is_some() => self.completed = true,
            _ => return Err(SnapshotError::Invalid),
        }
        Ok(())
    }

    pub fn finish(self) -> Result<terminal_diff::TerminalDiff, SnapshotError> {
        if !self.completed {
            return Err(SnapshotError::Invalid);
        }
        let header = self.header.ok_or(SnapshotError::Invalid)?;
        terminal_diff::TerminalDiff::try_new(header.title, header.notes, self.files)
    }
}

#[cfg(test)]
mod tests {
    use v1::read_terminal_diff_response::Event;

    use super::*;

    fn accept(decoder: &mut SnapshotDecoder, event: Event) -> Result<(), SnapshotError> {
        decoder.accept(v1::ReadTerminalDiffResponse { event: Some(event) })
    }
    fn started() -> SnapshotDecoder {
        let mut decoder = SnapshotDecoder::default();
        accept(
            &mut decoder,
            Event::Header(v1::TerminalDiffHeader {
                title: "Review".into(),
                notes: Vec::new(),
            }),
        )
        .unwrap();
        accept(
            &mut decoder,
            Event::File(v1::TerminalDiffFile {
                path: "a.rs".into(),
                added: 1,
                removed: 0,
            }),
        )
        .unwrap();
        decoder
    }
    fn rows(kind: i32, text: String, full_context: bool) -> Event {
        Event::Rows(v1::TerminalDiffRows {
            full_context,
            rows: vec![v1::TerminalDiffRow {
                kind,
                text,
                old_line_number: None,
                new_line_number: Some(1),
                syntax: Vec::new(),
            }],
        })
    }
    #[test]
    fn rejects_partial_out_of_order_and_unknown_stream_data() {
        assert_eq!(started().finish(), Err(SnapshotError::Invalid));
        let mut decoder = SnapshotDecoder::default();
        assert_eq!(
            accept(&mut decoder, rows(4, "new".into(), false)),
            Err(SnapshotError::Invalid)
        );
        let mut decoder = started();
        assert_eq!(
            accept(&mut decoder, rows(999, "new".into(), false)),
            Err(SnapshotError::Invalid)
        );
        let mut decoder = started();
        accept(&mut decoder, rows(4, "new".into(), true)).unwrap();
        assert_eq!(
            accept(&mut decoder, rows(4, "new".into(), false)),
            Err(SnapshotError::Invalid)
        );
    }
    #[test]
    fn rejects_oversized_source_and_duplicate_paths() {
        let mut decoder = started();
        assert_eq!(
            accept(
                &mut decoder,
                rows(4, "x".repeat(terminal_diff::ROW_BYTES_MAX + 1), false)
            ),
            Err(SnapshotError::TooLarge)
        );
        let mut decoder = started();
        accept(
            &mut decoder,
            Event::File(v1::TerminalDiffFile {
                path: "a.rs".into(),
                added: 0,
                removed: 0,
            }),
        )
        .unwrap();
        accept(&mut decoder, Event::Completed(v1::Empty {})).unwrap();
        assert_eq!(decoder.finish(), Err(SnapshotError::Invalid));
    }

    #[test]
    fn rejects_invalid_syntax_ranges_and_classes() {
        for (start, end, class) in [(0, 1, 1), (1, 2, 1), (0, 3, 1), (0, 2, 999)] {
            let mut decoder = started();
            let event = Event::Rows(v1::TerminalDiffRows {
                full_context: false,
                rows: vec![v1::TerminalDiffRow {
                    kind: v1::TerminalDiffRowKind::Added as i32,
                    text: "α".into(),
                    syntax: vec![v1::ViewerCodeSpan {
                        byte_start: start,
                        byte_end: end,
                        syntax_class: class,
                        changed: false,
                    }],
                    ..Default::default()
                }],
            });
            assert_eq!(accept(&mut decoder, event), Err(SnapshotError::Invalid));
        }
    }
}
