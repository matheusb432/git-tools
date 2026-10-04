use std::ops::Range;

use gtl_wire::{
    terminal_diff::{File, Row, RowKind, TerminalDiff},
    viewer::ViewerCodeSpan,
};
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

#[derive(Default)]
pub(super) struct Document {
    pub title: String,
    pub notes: Vec<String>,
    pub files: Vec<File>,
}

impl From<TerminalDiff> for Document {
    fn from(snapshot: TerminalDiff) -> Self {
        let (title, notes, mut files) = snapshot.into_parts();
        for file in &mut files {
            for row in file.compact.iter_mut().chain(&mut file.full) {
                sanitize_row(row);
            }
        }
        Self {
            title: printable(&title),
            notes: notes.iter().map(|note| printable(note)).collect(),
            files,
        }
    }
}

fn sanitize_row(row: &mut Row) {
    if row.text.bytes().all(|byte| matches!(byte, b' '..=b'~')) {
        return;
    }
    let mut text = String::with_capacity(row.text.len());
    let mut syntax = Vec::<ViewerCodeSpan>::new();
    let mut spans = row.syntax.iter().peekable();
    let mut column = 0;
    for (start, grapheme) in row.text.grapheme_indices(true) {
        while spans.peek().is_some_and(|span| span.byte_end <= start) {
            spans.next();
        }
        let class = spans.peek().and_then(|span| span.syntax_class);
        let byte_start = text.len();
        append_grapheme(&mut text, &mut column, grapheme);
        if let Some(last) = syntax.last_mut().filter(|last| last.syntax_class == class) {
            last.byte_end = text.len();
        } else {
            syntax.push(ViewerCodeSpan {
                byte_start,
                byte_end: text.len(),
                syntax_class: class,
                changed: false,
            });
        }
    }
    row.text = text;
    row.syntax = syntax;
}

pub(super) fn printable(text: &str) -> String {
    if !text.chars().any(char::is_control) {
        return text.to_owned();
    }
    let mut result = String::with_capacity(text.len());
    let mut column = 0;
    for grapheme in text.graphemes(true) {
        append_grapheme(&mut result, &mut column, grapheme);
    }
    result
}

fn append_grapheme(result: &mut String, column: &mut usize, grapheme: &str) {
    if grapheme == "\t" {
        let spaces = 4 - *column % 4;
        result.extend(std::iter::repeat_n(' ', spaces));
        *column += spaces;
    } else if grapheme.chars().any(char::is_control) {
        for character in grapheme.chars() {
            let escaped = character.escape_default();
            *column += escaped.len();
            result.extend(escaped);
        }
    } else {
        result.push_str(grapheme);
        *column += grapheme.width();
    }
}

pub(super) struct Entry {
    pub file: usize,
    pub line: Option<usize>,
    pub screen_start: usize,
    continuation_starts: Vec<usize>,
}

#[derive(Default)]
pub(super) struct Layout {
    pub entries: Vec<Entry>,
    pub files: Vec<usize>,
    pub hunks: Vec<usize>,
    pub screen_rows: usize,
    pub gutter: usize,
    pub full: bool,
    headers: Vec<String>,
}

#[derive(Clone)]
pub(super) struct Anchor {
    path: String,
    line: Option<(RowKind, Option<u32>, Option<u32>)>,
}

impl Anchor {
    fn matches(&self, row: &Row) -> bool {
        self.line.is_some_and(|(kind, old, new)| {
            row.kind == kind
                && (new.is_some() && new == row.new_line_number
                    || new.is_none() && old.is_some() && old == row.old_line_number)
        })
    }
}

impl Layout {
    pub fn new(document: &Document, width: usize, wrap: bool, full: bool) -> Self {
        let digits = document
            .files
            .iter()
            .flat_map(|file| file.rows(full))
            .flat_map(|row| [row.old_line_number, row.new_line_number])
            .flatten()
            .max()
            .unwrap_or(1)
            .to_string()
            .len();
        let gutter = if width >= 60 {
            digits * 2 + 5
        } else {
            digits + 4
        };
        let mut layout = Self {
            gutter,
            full,
            ..Self::default()
        };
        for (file_index, file) in document.files.iter().enumerate() {
            let header = format!(
                "{}  +{} -{}",
                printable(&file.path),
                file.added,
                file.removed
            );
            layout.files.push(layout.screen_rows);
            layout.push(file_index, None, &header, width, wrap);
            layout.headers.push(header);
            let mut previous_changed = false;
            for (line, row) in file
                .rows(full)
                .iter()
                .enumerate()
                .filter(|(_, row)| visible_row(row))
            {
                let changed = matches!(row.kind, RowKind::Added | RowKind::Removed);
                layout.hunks.extend(
                    ((!full && row.kind == RowKind::Hunk)
                        || (full && changed && !previous_changed))
                        .then_some(layout.screen_rows),
                );
                previous_changed = changed;
                layout.push(
                    file_index,
                    Some(line),
                    &row.text,
                    width.saturating_sub(gutter).max(1),
                    wrap && matches!(
                        row.kind,
                        RowKind::Context | RowKind::Added | RowKind::Removed
                    ),
                );
            }
        }
        layout
    }

    fn push(&mut self, file: usize, line: Option<usize>, text: &str, width: usize, wrap: bool) {
        let continuation_starts = if wrap {
            wrap_continuation_starts(text, width.max(1))
        } else {
            Vec::new()
        };
        let screen_start = self.screen_rows;
        self.screen_rows += continuation_starts.len() + 1;
        self.entries.push(Entry {
            file,
            line,
            screen_start,
            continuation_starts,
        });
    }

    pub fn entry_at(&self, offset: usize) -> Option<&Entry> {
        self.entries.get(
            self.entries
                .partition_point(|entry| entry.screen_start <= offset)
                .saturating_sub(1),
        )
    }

    pub fn source<'a>(&self, document: &'a Document, entry: &Entry) -> Option<&'a Row> {
        entry
            .line
            .map(|line| &document.files[entry.file].rows(self.full)[line])
    }

    pub fn text<'a>(&'a self, document: &'a Document, entry: &Entry) -> &'a str {
        self.source(document, entry).map_or_else(
            || self.headers[entry.file].as_str(),
            |row| row.text.as_str(),
        )
    }

    pub fn visible_range(&self, document: &Document, entry: &Entry, offset: usize) -> Range<usize> {
        let part = offset
            .saturating_sub(entry.screen_start)
            .min(entry.continuation_starts.len());
        part.checked_sub(1)
            .map_or(0, |part| entry.continuation_starts[part])
            ..entry
                .continuation_starts
                .get(part)
                .copied()
                .unwrap_or_else(|| self.text(document, entry).len())
    }

    pub fn anchor(&self, document: &Document, offset: usize) -> Option<Anchor> {
        let entry = self.entry_at(offset)?;
        let source = self.source(document, entry);
        let source = if source.is_some_and(|row| row.kind == RowKind::Hunk) {
            let next = self
                .entries
                .partition_point(|candidate| candidate.screen_start <= entry.screen_start);
            self.entries[next..]
                .iter()
                .take_while(|candidate| candidate.file == entry.file)
                .filter_map(|candidate| self.source(document, candidate))
                .find(|row| row.old_line_number.is_some() || row.new_line_number.is_some())
        } else {
            source
        };
        Some(Anchor {
            path: document.files[entry.file].path.clone(),
            line: source.map(|row| (row.kind, row.old_line_number, row.new_line_number)),
        })
    }

    pub fn locate(&self, document: &Document, anchor: &Anchor) -> Option<usize> {
        let file = document
            .files
            .iter()
            .position(|file| file.path == anchor.path)?;
        let exact = self.entries.iter().find(|entry| {
            entry.file == file
                && self
                    .source(document, entry)
                    .is_some_and(|row| anchor.matches(row))
        });
        exact
            .map(|entry| entry.screen_start)
            .or_else(|| self.files.get(file).copied())
    }
}

fn visible_row(row: &Row) -> bool {
    row.kind != RowKind::Meta
        || !["index ", "--- ", "+++ ", "diff --git "]
            .iter()
            .any(|prefix| row.text.starts_with(prefix))
}

fn wrap_continuation_starts(text: &str, width: usize) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut columns = 0;
    for (byte, grapheme) in text.grapheme_indices(true) {
        let size = grapheme.width();
        if columns + size > width && columns > 0 {
            starts.push(byte);
            columns = 0;
        }
        columns += size;
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_preserves_whitespace_and_graphemes() {
        let text = "  界e\u{301}👩‍💻 end";
        let document = Document {
            files: vec![File {
                path: "code.rs".into(),
                added: 0,
                removed: 0,
                review: None,
                compact: [text, "", "short"]
                    .into_iter()
                    .map(|text| Row {
                        kind: RowKind::Context,
                        text: text.into(),
                        old_line_number: None,
                        new_line_number: None,
                        syntax: Vec::new(),
                    })
                    .collect(),
                full: Vec::new(),
            }],
            ..Document::default()
        };
        for (width, wrap) in [(10, true), (100, true), (10, false)] {
            let layout = Layout::new(&document, width, wrap, false);
            assert_layout_source(&document, &layout, width, wrap);
        }
    }

    fn assert_layout_source(document: &Document, layout: &Layout, width: usize, wrap: bool) {
        for (index, entry) in layout.entries.iter().enumerate().skip(1) {
            let end = layout
                .entries
                .get(index + 1)
                .map_or(layout.screen_rows, |entry| entry.screen_start);
            let pieces = (entry.screen_start..end)
                .map(|offset| {
                    &layout.text(document, entry)[layout.visible_range(document, entry, offset)]
                })
                .collect::<Vec<_>>();
            assert_eq!(pieces.concat(), layout.text(document, entry));
            if wrap {
                assert!(
                    pieces
                        .iter()
                        .all(|piece| piece.width() <= width - layout.gutter)
                );
            } else {
                assert_eq!(pieces.len(), 1);
            }
        }
    }

    #[test]
    fn terminal_controls_are_visible_text_and_tabs_keep_columns() {
        assert_eq!(printable("界\tx\u{1b}[2J\r\0"), "界  x\\u{1b}[2J\\r\\u{0}");
        assert_eq!(printable("café\u{85}"), "café\\u{85}");
    }

    #[test]
    fn syntax_offsets_follow_tab_expansion_unicode_and_escaped_controls() {
        use gtl_wire::viewer::ViewerSyntaxClass;
        let text = "\tlet café = \"界e\u{301}\";\u{1b}";
        let string_start = text.find('"').unwrap();
        let string_end = text.rfind('"').unwrap() + 1;
        let mut row = Row {
            kind: RowKind::Added,
            text: text.into(),
            old_line_number: None,
            new_line_number: Some(1),
            syntax: [
                (0, 1, None),
                (1, 4, Some(ViewerSyntaxClass::Keyword)),
                (4, string_start, None),
                (string_start, string_end, Some(ViewerSyntaxClass::String)),
                (string_end, text.len(), None),
            ]
            .into_iter()
            .map(|(byte_start, byte_end, syntax_class)| ViewerCodeSpan {
                byte_start,
                byte_end,
                syntax_class,
                changed: false,
            })
            .collect(),
        };
        sanitize_row(&mut row);
        assert_eq!(row.text, "    let café = \"界e\u{301}\";\\u{1b}");
        assert!(row.syntax.iter().all(|span| span.text(&row.text).is_some()));
        assert_eq!(
            row.syntax
                .iter()
                .find(|span| span.syntax_class == Some(ViewerSyntaxClass::Keyword))
                .unwrap()
                .text(&row.text),
            Some("let")
        );
        assert_eq!(
            row.syntax
                .iter()
                .find(|span| span.syntax_class == Some(ViewerSyntaxClass::String))
                .unwrap()
                .text(&row.text),
            Some("\"界e\u{301}\"")
        );
    }
}
