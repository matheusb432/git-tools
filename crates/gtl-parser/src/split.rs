use crate::{
    CharacterSpan, DiffRow, DiffRowKind, SyntaxToken,
    intraline::{ChangedLineSpans, changed_spans},
};

/// One populated side of a paired side-by-side diff row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitDiffCell {
    line_number: u32,
    text: String,
    syntax_tokens: Vec<SyntaxToken>,
    intraline_spans: Vec<CharacterSpan>,
    long_line_character_count: Option<usize>,
}

impl SplitDiffCell {
    /// Returns the line number for this side.
    pub const fn line_number(&self) -> u32 {
        self.line_number
    }

    /// Returns the raw diff line, including its leading marker.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns semantic syntax tokens indexed over the marker-free body.
    pub fn syntax_tokens(&self) -> &[SyntaxToken] {
        &self.syntax_tokens
    }

    /// Returns intraline changed spans indexed over the marker-free body.
    pub fn intraline_spans(&self) -> &[CharacterSpan] {
        &self.intraline_spans
    }

    /// Returns the source character count when this cell exceeds the parser limit.
    pub const fn long_line_character_count(&self) -> Option<usize> {
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
        old_line_number: u32,
        new_line_number: u32,
        text: String,
        syntax_tokens: Vec<SyntaxToken>,
        long_line_character_count: Option<usize>,
    },
    Pair {
        old: Option<SplitDiffCell>,
        new: Option<SplitDiffCell>,
    },
}

pub(crate) fn split_rows(rows: &[DiffRow]) -> Vec<SplitDiffRow> {
    let mut output = Vec::with_capacity(rows.len());
    let mut removed = Vec::new();
    let mut added = Vec::new();

    for row in rows {
        match row.kind() {
            DiffRowKind::Removed => removed.push(row),
            DiffRowKind::Added => added.push(row),
            DiffRowKind::Meta => {
                flush_pairs(&mut output, &mut removed, &mut added);
                output.push(SplitDiffRow::Meta {
                    text: row.text().to_owned(),
                });
            }
            DiffRowKind::Hunk => {
                flush_pairs(&mut output, &mut removed, &mut added);
                output.push(SplitDiffRow::Hunk {
                    text: row.text().to_owned(),
                });
            }
            DiffRowKind::Context => {
                flush_pairs(&mut output, &mut removed, &mut added);
                output.push(SplitDiffRow::Context {
                    old_line_number: row.old_line_number().unwrap_or(0),
                    new_line_number: row.new_line_number().unwrap_or(0),
                    text: row.text().to_owned(),
                    syntax_tokens: row.syntax_tokens().to_vec(),
                    long_line_character_count: row.long_line_character_count(),
                });
            }
        }
    }
    flush_pairs(&mut output, &mut removed, &mut added);

    output
}

fn flush_pairs(
    output: &mut Vec<SplitDiffRow>,
    removed: &mut Vec<&DiffRow>,
    added: &mut Vec<&DiffRow>,
) {
    for index in 0..removed.len().max(added.len()) {
        let old = removed.get(index).copied();
        let new = added.get(index).copied();

        let spans = match (old, new) {
            (Some(old), Some(new))
                if old.long_line_character_count().is_none()
                    && new.long_line_character_count().is_none() =>
            {
                changed_spans(old.body(), new.body())
            }
            _ => ChangedLineSpans::default(),
        };

        output.push(SplitDiffRow::Pair {
            old: old.map(|row| split_cell(row, spans.old.clone(), true)),
            new: new.map(|row| split_cell(row, spans.new.clone(), false)),
        });
    }
    removed.clear();
    added.clear();
}

fn split_cell(row: &DiffRow, intraline_spans: Vec<CharacterSpan>, old: bool) -> SplitDiffCell {
    SplitDiffCell {
        line_number: if old {
            row.old_line_number().unwrap_or(0)
        } else {
            row.new_line_number().unwrap_or(0)
        },
        text: row.text().to_owned(),
        syntax_tokens: row.syntax_tokens().to_vec(),
        intraline_spans,
        long_line_character_count: row.long_line_character_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiffParser, ParseOptions};

    fn split(raw: &[&str]) -> Vec<SplitDiffRow> {
        let lines = raw.iter().map(ToString::to_string).collect::<Vec<_>>();
        DiffParser::new().parse(&lines).split_rows()
    }

    #[test]
    fn pairs_removed_and_added_runs_indexwise_with_padding() {
        let rows = split(&["@@ -1,3 +1,2 @@", "-a", "-b", "+c"]);

        assert!(matches!(rows[0], SplitDiffRow::Hunk { .. }));
        let SplitDiffRow::Pair {
            old: Some(old),
            new: Some(new),
        } = &rows[1]
        else {
            panic!("first pair row must have both sides");
        };
        assert_eq!((old.line_number(), old.text()), (1, "-a"));
        assert_eq!((new.line_number(), new.text()), (1, "+c"));

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
        let SplitDiffRow::Context {
            old_line_number,
            new_line_number,
            text,
            ..
        } = &rows[2]
        else {
            panic!("context row expected");
        };
        assert_eq!(
            (*old_line_number, *new_line_number, text.as_str()),
            (2, 1, " mid")
        );
    }

    #[test]
    fn paired_short_lines_receive_intraline_spans() {
        let rows = split(&["@@ -1 +1 @@", "-let x = 1;", "+let x = 2;"]);
        let SplitDiffRow::Pair {
            old: Some(old),
            new: Some(new),
        } = &rows[1]
        else {
            panic!("pair expected");
        };

        assert_eq!(old.intraline_spans(), [CharacterSpan::new(8, 9)]);
        assert_eq!(new.intraline_spans(), [CharacterSpan::new(8, 9)]);
    }

    #[test]
    fn long_pairs_skip_intraline_parsing() {
        let lines = ["@@ -1 +1 @@", "-aaaa", "+aaab"]
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let rows = DiffParser::with_options(ParseOptions::new(3))
            .parse(&lines)
            .split_rows();
        let SplitDiffRow::Pair {
            old: Some(old),
            new: Some(new),
        } = &rows[1]
        else {
            panic!("pair expected");
        };

        assert!(old.intraline_spans().is_empty());
        assert!(new.intraline_spans().is_empty());
    }

    #[test]
    fn unpaired_and_trailing_runs_are_retained_without_spans() {
        let rows = split(&["@@ -1,2 +1 @@", "-a", "-b", "+c"]);
        let SplitDiffRow::Pair {
            old: Some(old),
            new: None,
        } = &rows[2]
        else {
            panic!("second removed line should be retained");
        };

        assert!(old.intraline_spans().is_empty());
        assert_eq!(old.text(), "-b");
    }
}
