use std::{mem, ops::Range};

use crate::{
    DiffRow, DiffRowKind, DiffSide, SyntaxDiagnostic, SyntaxHunkByteLimit, SyntaxLanguage,
    SyntaxToken,
    syntax::{ByteSyntaxToken, SideHighlighter},
};

pub(crate) struct DiffSyntaxHighlighter {
    language: SyntaxLanguage,
    max_hunk_bytes: SyntaxHunkByteLimit,
    old_side: SideHighlighter,
    new_side: SideHighlighter,
    state: HunkState,
    disabled: bool,
}

impl DiffSyntaxHighlighter {
    pub(crate) fn new(language: SyntaxLanguage, max_hunk_bytes: SyntaxHunkByteLimit) -> Self {
        Self {
            language,
            max_hunk_bytes,
            old_side: SideHighlighter::new(),
            new_side: SideHighlighter::new(),
            state: HunkState::BetweenHunks,
            disabled: false,
        }
    }

    pub(crate) fn push(&mut self, rows: Vec<DiffRow>) -> SyntaxBatch {
        let mut output = SyntaxBatch::default();

        for row in rows {
            self.push_row(row, &mut output);
        }

        output
    }

    fn push_row(&mut self, row: DiffRow, output: &mut SyntaxBatch) {
        if self.disabled {
            output.rows.push(row);
            return;
        }

        if row.kind() == DiffRowKind::Hunk {
            self.start_hunk(row, output);
            return;
        }

        let oversized_side = match &mut self.state {
            HunkState::BetweenHunks | HunkState::PassThrough => {
                output.rows.push(row);
                None
            }
            HunkState::Collecting(hunk) => hunk.push(row, self.max_hunk_bytes),
        };
        if let Some(side) = oversized_side {
            self.pass_through_oversized_hunk(side, output);
        }
    }

    fn start_hunk(&mut self, row: DiffRow, output: &mut SyntaxBatch) {
        self.flush(output);
        if self.disabled {
            output.rows.push(row);
        } else {
            self.state = HunkState::Collecting(HunkBuffer::new(row));
        }
    }

    fn pass_through_oversized_hunk(&mut self, side: DiffSide, output: &mut SyntaxBatch) {
        let HunkState::Collecting(hunk) = mem::replace(&mut self.state, HunkState::PassThrough)
        else {
            return;
        };
        output.rows.extend(hunk.rows);
        output.diagnostics.push(SyntaxDiagnostic::new(
            side,
            format!(
                "syntax hunk {side:?} source exceeds the {}-byte limit",
                self.max_hunk_bytes.into_inner()
            ),
        ));
    }

    pub(crate) fn finish(mut self) -> SyntaxBatch {
        let mut output = SyntaxBatch::default();
        self.flush(&mut output);
        output
    }

    fn flush(&mut self, output: &mut SyntaxBatch) {
        let HunkState::Collecting(mut hunk) =
            mem::replace(&mut self.state, HunkState::BetweenHunks)
        else {
            return;
        };

        match self.attach_hunk(&mut hunk.rows) {
            Ok(()) => output.rows.extend(hunk.rows),
            Err(failure) => {
                self.disabled = true;
                output.rows.extend(hunk.rows);
                output
                    .diagnostics
                    .push(SyntaxDiagnostic::new(failure.side, failure.message));
            }
        }
    }

    fn attach_hunk(&mut self, rows: &mut [DiffRow]) -> Result<(), HighlightFailure> {
        let (old_source, new_source) = reconstruct_sources(rows);
        let mut old_tokens = self.highlight_source(DiffSide::Old, &old_source, rows.len())?;
        let mut new_tokens = self.highlight_source(DiffSide::New, &new_source, rows.len())?;

        for (row_index, row) in rows.iter_mut().enumerate() {
            let tokens = match row.kind() {
                DiffRowKind::Removed => mem::take(&mut old_tokens[row_index]),
                DiffRowKind::Added | DiffRowKind::Context => mem::take(&mut new_tokens[row_index]),
                DiffRowKind::Meta | DiffRowKind::Hunk => Vec::new(),
            };
            row.set_syntax_tokens(tokens);
        }
        Ok(())
    }

    fn highlight_source(
        &mut self,
        side: DiffSide,
        source: &HunkSource,
        row_count: usize,
    ) -> Result<Vec<Vec<SyntaxToken>>, HighlightFailure> {
        let byte_tokens = match side {
            DiffSide::Old => self.old_side.tokens(self.language, &source.text, None),
            DiffSide::New => self.new_side.tokens(self.language, &source.text, None),
        }
        .map_err(|message| HighlightFailure { side, message })?;

        tokens_by_row(&source.text, &source.rows, &byte_tokens, row_count)
            .map_err(|message| HighlightFailure { side, message })
    }
}

#[derive(Default)]
pub(crate) struct SyntaxBatch {
    pub(crate) rows: Vec<DiffRow>,
    pub(crate) diagnostics: Vec<SyntaxDiagnostic>,
}

enum HunkState {
    BetweenHunks,
    Collecting(HunkBuffer),
    PassThrough,
}

struct HunkBuffer {
    rows: Vec<DiffRow>,
    old_source_bytes: usize,
    new_source_bytes: usize,
}

impl HunkBuffer {
    fn new(header: DiffRow) -> Self {
        Self {
            rows: vec![header],
            old_source_bytes: 0,
            new_source_bytes: 0,
        }
    }

    fn push(&mut self, row: DiffRow, max_hunk_bytes: SyntaxHunkByteLimit) -> Option<DiffSide> {
        let source_bytes = row.body().len().saturating_add(1);
        match row.kind() {
            DiffRowKind::Removed => {
                self.old_source_bytes = self.old_source_bytes.saturating_add(source_bytes);
            }
            DiffRowKind::Added => {
                self.new_source_bytes = self.new_source_bytes.saturating_add(source_bytes);
            }
            DiffRowKind::Context => {
                self.old_source_bytes = self.old_source_bytes.saturating_add(source_bytes);
                self.new_source_bytes = self.new_source_bytes.saturating_add(source_bytes);
            }
            DiffRowKind::Meta | DiffRowKind::Hunk => {}
        }
        self.rows.push(row);

        let max_hunk_bytes = max_hunk_bytes.into_inner();
        if self.old_source_bytes > max_hunk_bytes {
            Some(DiffSide::Old)
        } else if self.new_source_bytes > max_hunk_bytes {
            Some(DiffSide::New)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub(crate) struct HunkSource {
    pub(crate) text: String,
    pub(crate) rows: Vec<SourceRowRange>,
}

impl HunkSource {
    fn push(&mut self, row_index: usize, row: &DiffRow) {
        let start = self.text.len();
        self.text.push_str(row.body());
        let end = self.text.len();
        self.text.push('\n');
        self.rows.push(SourceRowRange {
            row_index,
            byte_range: start..end,
            suppress_tokens: row.long_line_character_count().is_some(),
        });
    }
}

pub(crate) struct SourceRowRange {
    pub(crate) row_index: usize,
    pub(crate) byte_range: Range<usize>,
    pub(crate) suppress_tokens: bool,
}

struct HighlightFailure {
    side: DiffSide,
    message: String,
}

fn reconstruct_sources(rows: &[DiffRow]) -> (HunkSource, HunkSource) {
    let mut old_source = HunkSource::default();
    let mut new_source = HunkSource::default();

    for (row_index, row) in rows.iter().enumerate() {
        match row.kind() {
            DiffRowKind::Removed => old_source.push(row_index, row),
            DiffRowKind::Added => new_source.push(row_index, row),
            DiffRowKind::Context => {
                old_source.push(row_index, row);
                new_source.push(row_index, row);
            }
            DiffRowKind::Meta | DiffRowKind::Hunk => {}
        }
    }
    (old_source, new_source)
}

pub(crate) fn tokens_by_row(
    text: &str,
    rows: &[SourceRowRange],
    byte_tokens: &[ByteSyntaxToken],
    row_count: usize,
) -> Result<Vec<Vec<SyntaxToken>>, String> {
    let mut output = vec![Vec::new(); row_count];
    let mut first_candidate = 0usize;

    for token in byte_tokens {
        while rows
            .get(first_candidate)
            .is_some_and(|row| row.byte_range.end <= token.start)
        {
            first_candidate += 1;
        }

        append_token_to_rows(text, rows, token, first_candidate, &mut output)?;
    }

    Ok(output)
}

fn append_token_to_rows(
    text: &str,
    rows: &[SourceRowRange],
    token: &ByteSyntaxToken,
    first_candidate: usize,
    output: &mut [Vec<SyntaxToken>],
) -> Result<(), String> {
    for row in rows.iter().skip(first_candidate) {
        if row.byte_range.start >= token.end {
            break;
        }
        if row.suppress_tokens {
            continue;
        }

        let start = token.start.max(row.byte_range.start);
        let end = token.end.min(row.byte_range.end);
        if start >= end {
            continue;
        }
        let prefix = text
            .get(row.byte_range.start..start)
            .ok_or_else(|| "highlight start is not on a UTF-8 boundary".to_owned())?;
        let text = text
            .get(start..end)
            .ok_or_else(|| "highlight end is not on a UTF-8 boundary".to_owned())?;
        let character_start = prefix.chars().count();
        let character_end = character_start + text.chars().count();
        push_syntax_token(
            &mut output[row.row_index],
            character_start,
            character_end,
            token.class,
        );
    }
    Ok(())
}

fn push_syntax_token(
    tokens: &mut Vec<SyntaxToken>,
    start: usize,
    end: usize,
    class: crate::SyntaxTokenClass,
) {
    match tokens.last_mut() {
        Some(last) if last.end().into_inner() == start && last.class() == class => {
            *last = SyntaxToken::new(last.start().into_inner(), end, class);
        }
        _ => tokens.push(SyntaxToken::new(start, end, class)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CharacterCount, DiffParser, DiffRowKind, ParseOptions, SyntaxHunkByteLimit,
        SyntaxTokenClass,
    };

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    fn rust_parser() -> DiffParser {
        DiffParser::new().with_syntax(Some(SyntaxLanguage::Rust))
    }

    fn token_overlaps(row: &DiffRow, needle: &str, class: SyntaxTokenClass) -> bool {
        let start = row.body().find(needle).unwrap();
        let start = row.body()[..start].chars().count();
        let end = start + needle.chars().count();
        row.syntax_tokens().iter().any(|token| {
            token.class() == class
                && token.start().into_inner() < end
                && start < token.end().into_inner()
        })
    }

    #[test]
    fn completed_hunk_preserves_multiline_comment_context_on_both_sides() {
        let parsed = rust_parser().parse(lines(&[
            "@@ -1,4 +1,4 @@",
            " /* open",
            "-old inside",
            "+new inside",
            " */",
        ]));

        for row in &parsed.rows()[2..=3] {
            assert!(matches!(
                row.kind(),
                DiffRowKind::Removed | DiffRowKind::Added
            ));
            assert!(!row.syntax_tokens().is_empty());
            assert!(
                row.syntax_tokens()
                    .iter()
                    .all(|token| token.class() == SyntaxTokenClass::Comment)
            );
        }
    }

    #[test]
    fn hunk_buffer_crosses_push_boundaries_and_finish_emits_it() {
        let mut stream = rust_parser().stream();
        let first = stream.push(lines(&["@@ -1,4 +1,4 @@", " /* open"]));
        let second = stream.push(lines(&["-old inside", "+new inside", " */"]));

        assert!(first.rows().is_empty());
        assert!(second.rows().is_empty());
        let finished = stream.finish();
        assert_eq!(finished.rows().len(), 5);
        assert!(finished.syntax_diagnostics().is_empty());
        for row in &finished.rows()[2..=3] {
            assert!(
                row.syntax_tokens()
                    .iter()
                    .all(|token| token.class() == SyntaxTokenClass::Comment)
            );
        }
    }

    #[test]
    fn next_hunk_flushes_the_completed_hunk() {
        let mut stream = rust_parser().stream();
        let batch = stream.push(lines(&[
            "@@ -1 +1 @@",
            " let first = 1;",
            "@@ -4 +4 @@",
            " let second = 2;",
        ]));

        assert_eq!(batch.rows().len(), 2);
        assert_eq!(batch.rows()[0].kind(), DiffRowKind::Hunk);
        assert!(
            batch.rows()[1]
                .syntax_tokens()
                .iter()
                .any(|token| token.class() == SyntaxTokenClass::Keyword)
        );
        assert_eq!(stream.finish().rows().len(), 2);
    }

    #[test]
    fn overflowing_hunk_passes_through_and_next_hunk_recovers() {
        let parser = DiffParser::with_options(
            ParseOptions::new(CharacterCount::new(2_000))
                .with_max_syntax_hunk_bytes(SyntaxHunkByteLimit::new(10)),
        )
        .with_syntax(Some(SyntaxLanguage::Rust));
        let parsed = parser.parse(lines(&[
            "@@ -1 +1 @@",
            " let value = 1;",
            "@@ -3 +3 @@",
            " let x=1;",
        ]));

        assert_eq!(parsed.syntax_diagnostics().len(), 1);
        assert!(parsed.rows()[1].syntax_tokens().is_empty());
        assert!(
            parsed.rows()[3]
                .syntax_tokens()
                .iter()
                .any(|token| token.class() == SyntaxTokenClass::Keyword)
        );
    }

    #[test]
    fn rust_macros_raw_identifiers_and_multi_hash_strings_keep_distinct_classes() {
        let parsed = rust_parser().parse(lines(&[
            "@@ -1,6 +1,6 @@",
            " macro_rules! passthrough { ($($tokens:tt)*) => { $($tokens)* }; }",
            " passthrough! {",
            "     async fn run() {",
            "         let r#async = r##\"async \"# content\"##;",
            "     }",
            " }",
        ]));

        let async_function = &parsed.rows()[3];
        let raw_identifier_and_string = &parsed.rows()[4];
        assert!(token_overlaps(
            async_function,
            "async",
            SyntaxTokenClass::Keyword
        ));
        assert!(!token_overlaps(
            raw_identifier_and_string,
            "r#async",
            SyntaxTokenClass::Keyword
        ));
        let string_start = raw_identifier_and_string.body().find("r##\"").unwrap();
        let async_in_string = raw_identifier_and_string.body()[string_start..]
            .find("async")
            .unwrap()
            + string_start;
        let async_in_string = raw_identifier_and_string.body()[..async_in_string]
            .chars()
            .count();
        assert!(
            raw_identifier_and_string
                .syntax_tokens()
                .iter()
                .any(|token| {
                    token.class() == SyntaxTokenClass::String
                        && token.start().into_inner() <= async_in_string
                        && async_in_string < token.end().into_inner()
                })
        );
        assert!(parsed.syntax_diagnostics().is_empty());
    }

    #[test]
    fn unicode_byte_ranges_map_to_row_character_offsets() {
        let parsed = rust_parser().parse(lines(&["@@ -1 +1 @@", " let café = \"α\";"]));
        let string = parsed.rows()[1]
            .syntax_tokens()
            .iter()
            .find(|token| token.class() == SyntaxTokenClass::String)
            .unwrap();

        assert_eq!(string.start().into_inner(), 11);
        assert_eq!(string.end().into_inner(), 14);
    }

    #[test]
    fn overlong_rows_feed_parser_context_without_receiving_tokens() {
        let parser = DiffParser::with_options(ParseOptions::new(CharacterCount::new(4)))
            .with_syntax(Some(SyntaxLanguage::Rust));
        let parsed = parser.parse(lines(&[
            "@@ -1,2 +1,2 @@",
            " /* comment begins beyond the row limit",
            " x",
        ]));

        assert!(parsed.rows()[1].syntax_tokens().is_empty());
        assert!(token_overlaps(
            &parsed.rows()[2],
            "x",
            SyntaxTokenClass::Comment
        ));
    }

    #[test]
    fn malformed_rust_keeps_valid_captures_without_a_diagnostic() {
        let parsed = rust_parser().parse(lines(&[
            "@@ -1,2 +1,2 @@",
            " let broken = ;",
            " async fn valid() {}",
        ]));

        assert!(token_overlaps(
            &parsed.rows()[1],
            "let",
            SyntaxTokenClass::Keyword
        ));
        assert!(token_overlaps(
            &parsed.rows()[2],
            "async",
            SyntaxTokenClass::Keyword
        ));
        assert!(parsed.syntax_diagnostics().is_empty());
    }

    #[test]
    fn supported_injections_highlight_and_unsupported_css_stays_plain() {
        let markdown = DiffParser::new()
            .with_syntax(Some(SyntaxLanguage::Markdown))
            .parse(lines(&[
                "@@ -1,3 +1,3 @@",
                " ```rust",
                " async fn run() {}",
                " ```",
            ]));
        assert!(token_overlaps(
            &markdown.rows()[2],
            "async",
            SyntaxTokenClass::Keyword
        ));

        let html = DiffParser::new()
            .with_syntax(Some(SyntaxLanguage::Html))
            .parse(lines(&[
                "@@ -1,2 +1,2 @@",
                " <script>const value = 1;</script>",
                " <style>color: red;</style>",
            ]));
        assert!(token_overlaps(
            &html.rows()[1],
            "const",
            SyntaxTokenClass::Keyword
        ));
        assert!(!html.rows()[2].syntax_tokens().iter().any(|token| {
            let color_start = html.rows()[2].body().find("color").unwrap();
            token.start().into_inner() < color_start + "color".len()
                && color_start < token.end().into_inner()
        }));
    }

    #[test]
    fn new_side_overflow_reports_once_and_recovers_at_the_next_hunk() {
        let parser = DiffParser::with_options(
            ParseOptions::default().with_max_syntax_hunk_bytes(SyntaxHunkByteLimit::new(10)),
        )
        .with_syntax(Some(SyntaxLanguage::Rust));
        let parsed = parser.parse(lines(&[
            "@@ -0,0 +1 @@",
            "+let added_value = 1;",
            "+let more = 2;",
            "@@ -0,0 +4 @@",
            "+let x=1;",
        ]));

        assert_eq!(parsed.syntax_diagnostics().len(), 1);
        assert_eq!(parsed.syntax_diagnostics()[0].side(), DiffSide::New);
        assert!(parsed.rows()[1].syntax_tokens().is_empty());
        assert!(token_overlaps(
            &parsed.rows()[4],
            "let",
            SyntaxTokenClass::Keyword
        ));
    }
}
