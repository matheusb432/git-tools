use crate::{
    CharacterCount, CharacterSpan, DiffRow, DiffRowKind, SemanticTextSpan, SourceLineNumber,
    SyntaxToken,
    intraline::{ChangedLineSpans, changed_spans},
    semantic::semantic_text_spans,
};

/// One populated side of a paired side-by-side diff row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitDiffCell {
    line_number: SourceLineNumber,
    text: String,
    syntax_tokens: Vec<SyntaxToken>,
    intraline_spans: Vec<CharacterSpan>,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<CharacterCount>,
}

impl SplitDiffCell {
    /// Returns the line number for this side.
    #[must_use]
    pub const fn line_number(&self) -> SourceLineNumber {
        self.line_number
    }

    /// Returns the raw diff line, including its leading marker.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns semantic syntax tokens indexed over the marker-free body.
    #[must_use]
    pub fn syntax_tokens(&self) -> &[SyntaxToken] {
        &self.syntax_tokens
    }

    /// Returns intraline changed spans indexed over the marker-free body.
    #[must_use]
    pub fn intraline_spans(&self) -> &[CharacterSpan] {
        &self.intraline_spans
    }

    /// Returns flat syntax and intraline spans over [`Self::body`].
    #[must_use]
    pub fn semantic_spans(&self) -> &[SemanticTextSpan] {
        &self.semantic_spans
    }

    /// Returns the marker-free source text.
    #[must_use]
    pub fn body(&self) -> &str {
        crate::diff_line_body(&self.text)
    }

    /// Returns the source character count when this cell exceeds the parser limit.
    #[must_use]
    pub const fn long_line_character_count(&self) -> Option<CharacterCount> {
        self.long_line_character_count
    }
}

/// One semantic row for a side-by-side diff presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitDiffRow {
    Meta {
        text: String,
    },
    Hunk {
        text: String,
    },
    Context {
        old_line_number: SourceLineNumber,
        new_line_number: SourceLineNumber,
        text: String,
        syntax_tokens: Vec<SyntaxToken>,
        semantic_spans: Vec<SemanticTextSpan>,
        long_line_character_count: Option<CharacterCount>,
    },
    Pair {
        old: Option<SplitDiffCell>,
        new: Option<SplitDiffCell>,
    },
}

/// Incremental side-by-side row derivation for one parsed file.
#[derive(Debug, Default)]
pub struct SplitDiffStream {
    removed: Vec<DiffRow>,
    added: Vec<DiffRow>,
}

impl SplitDiffStream {
    /// Starts an empty side-by-side stream.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            removed: Vec::new(),
            added: Vec::new(),
        }
    }

    /// Accepts parsed rows and returns every newly stable split row.
    pub fn push(&mut self, rows: impl IntoIterator<Item = DiffRow>) -> Vec<SplitDiffRow> {
        let mut output = Vec::new();

        for row in rows {
            self.push_row(row, &mut output);
        }

        output
    }

    fn push_row(&mut self, row: DiffRow, output: &mut Vec<SplitDiffRow>) {
        match row.kind() {
            DiffRowKind::Removed => self.removed.push(row),
            DiffRowKind::Added => self.added.push(row),
            DiffRowKind::Meta | DiffRowKind::Hunk | DiffRowKind::Context => {
                self.flush_pairs(output);
                output.push(single_row(&row));
            }
        }
    }

    /// Flushes a trailing change run and completes the stream.
    #[must_use]
    pub fn finish(mut self) -> Vec<SplitDiffRow> {
        let mut output = Vec::new();
        self.flush_pairs(&mut output);
        output
    }

    fn flush_pairs(&mut self, output: &mut Vec<SplitDiffRow>) {
        for index in 0..self.removed.len().max(self.added.len()) {
            let old = self.removed.get(index);
            let new = self.added.get(index);

            output.push(paired_row(old, new));
        }
        self.removed.clear();
        self.added.clear();
    }
}

pub(crate) fn paired_row(old: Option<&DiffRow>, new: Option<&DiffRow>) -> SplitDiffRow {
    let spans = changed_line_spans(old, new);
    SplitDiffRow::Pair {
        old: old.map(|row| split_cell(row, spans.old, true)),
        new: new.map(|row| split_cell(row, spans.new, false)),
    }
}

pub(crate) fn single_row(row: &DiffRow) -> SplitDiffRow {
    match row.kind() {
        DiffRowKind::Meta => SplitDiffRow::Meta {
            text: row.text().to_owned(),
        },
        DiffRowKind::Hunk => SplitDiffRow::Hunk {
            text: row.text().to_owned(),
        },
        DiffRowKind::Context => SplitDiffRow::Context {
            old_line_number: row.old_line_number().unwrap_or_default(),
            new_line_number: row.new_line_number().unwrap_or_default(),
            text: row.text().to_owned(),
            syntax_tokens: row.syntax_tokens().to_vec(),
            semantic_spans: row.semantic_spans().to_vec(),
            long_line_character_count: row.long_line_character_count(),
        },
        DiffRowKind::Added => paired_row(None, Some(row)),
        DiffRowKind::Removed => paired_row(Some(row), None),
    }
}

fn changed_line_spans(old: Option<&DiffRow>, new: Option<&DiffRow>) -> ChangedLineSpans {
    match (old, new) {
        (Some(old), Some(new))
            if old.long_line_character_count().is_none()
                && new.long_line_character_count().is_none() =>
        {
            changed_spans(old.body(), new.body())
        }
        _ => ChangedLineSpans::default(),
    }
}

pub(crate) fn split_rows(rows: &[DiffRow]) -> Vec<SplitDiffRow> {
    let mut stream = SplitDiffStream::new();
    let mut output = stream.push(rows.iter().cloned());
    output.extend(stream.finish());
    output
}

fn split_cell(row: &DiffRow, intraline_spans: Vec<CharacterSpan>, old: bool) -> SplitDiffCell {
    let semantic_spans = semantic_text_spans(row.body(), row.syntax_tokens(), &intraline_spans);
    SplitDiffCell {
        line_number: if old {
            row.old_line_number().unwrap_or_default()
        } else {
            row.new_line_number().unwrap_or_default()
        },
        text: row.text().to_owned(),
        syntax_tokens: row.syntax_tokens().to_vec(),
        intraline_spans,
        semantic_spans,
        long_line_character_count: row.long_line_character_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiffParser, ParseOptions};

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    fn split(raw: &[&str]) -> Vec<SplitDiffRow> {
        DiffParser::new().parse(lines(raw)).split_rows()
    }

    #[test]
    fn pairs_removed_and_added_runs_indexwise_with_padding() {
        let rows = split(&["@@ -1,3 +1,2 @@", "-a", "-b", "+c"]);

        assert!(matches!(rows[0], SplitDiffRow::Hunk { .. }));
        let (old, new) = match &rows[1] {
            SplitDiffRow::Pair {
                old: Some(old),
                new: Some(new),
            } => Some((old, new)),
            _ => None,
        }
        .unwrap();
        assert_eq!(
            (old.line_number(), old.text()),
            (SourceLineNumber::new(1), "-a")
        );
        assert_eq!(
            (new.line_number(), new.text()),
            (SourceLineNumber::new(1), "+c")
        );

        assert!(matches!(
            rows[2],
            SplitDiffRow::Pair {
                old: Some(_),
                new: None
            }
        ));
    }

    #[test]
    fn context_rows_flush_runs_and_mirror_line_numbers() {
        let rows = split(&["@@ -1,3 +1,3 @@", "-a", " mid", "+b"]);

        assert!(matches!(
            rows[1],
            SplitDiffRow::Pair {
                old: Some(_),
                new: None
            }
        ));
        let (old_line_number, new_line_number, text) = match &rows[2] {
            SplitDiffRow::Context {
                old_line_number,
                new_line_number,
                text,
                ..
            } => Some((old_line_number, new_line_number, text)),
            _ => None,
        }
        .unwrap();
        assert_eq!(
            (*old_line_number, *new_line_number, text.as_str()),
            (SourceLineNumber::new(2), SourceLineNumber::new(1), " mid")
        );
    }

    #[test]
    fn paired_short_lines_receive_intraline_spans() {
        let rows = split(&["@@ -1 +1 @@", "-let x = 1;", "+let x = 2;"]);
        let (old, new) = match &rows[1] {
            SplitDiffRow::Pair {
                old: Some(old),
                new: Some(new),
            } => Some((old, new)),
            _ => None,
        }
        .unwrap();

        assert_eq!(old.intraline_spans(), [CharacterSpan::new(8, 9)]);
        assert_eq!(new.intraline_spans(), [CharacterSpan::new(8, 9)]);
    }

    #[test]
    fn long_pairs_skip_intraline_parsing() {
        let lines = ["@@ -1 +1 @@", "-aaaa", "+aaab"]
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let rows = DiffParser::with_options(ParseOptions::new(CharacterCount::new(3)))
            .parse(&lines)
            .split_rows();
        let (old, new) = match &rows[1] {
            SplitDiffRow::Pair {
                old: Some(old),
                new: Some(new),
            } => Some((old, new)),
            _ => None,
        }
        .unwrap();

        assert_eq!(old.intraline_spans(), []);
        assert_eq!(new.intraline_spans(), []);
    }

    #[test]
    fn unpaired_and_trailing_runs_are_retained_without_spans() {
        let rows = split(&["@@ -1,2 +1 @@", "-a", "-b", "+c"]);
        let old = match &rows[2] {
            SplitDiffRow::Pair {
                old: Some(old),
                new: None,
            } => Some(old),
            _ => None,
        }
        .unwrap();

        assert_eq!(old.intraline_spans(), []);
        assert_eq!(old.text(), "-b");
    }

    #[test]
    fn streaming_split_buffers_only_an_unresolved_change_run() {
        let source = lines(&[
            "@@ -1,4 +1,3 @@",
            "-old one",
            "-old two",
            "+new one",
            " context",
        ]);
        let parsed = DiffParser::new().parse(&source);
        let expected = parsed.split_rows();
        let mut stream = SplitDiffStream::new();
        let mut actual = stream.push(parsed.rows()[..2].iter().cloned());

        assert!(matches!(actual.as_slice(), [SplitDiffRow::Hunk { .. }]));
        assert_eq!(
            stream.push(parsed.rows()[2..4].iter().cloned()),
            Vec::<SplitDiffRow>::new()
        );
        actual.extend(stream.push(parsed.rows()[4..].iter().cloned()));
        actual.extend(stream.finish());

        assert_eq!(actual, expected);
    }
}
