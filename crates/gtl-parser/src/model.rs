use crate::{
    CharacterCount, LineNumberDigitWidth, SemanticTextSpan, SourceLineNumber, SyntaxHunkByteLimit,
    SyntaxToken, UnifiedDiffLineClassifier, UnifiedDiffLineKind, semantic::semantic_text_spans,
};
#[cfg(feature = "syntax")]
use crate::{SyntaxLanguage, highlight::DiffSyntaxHighlighter};

/// Default source-line character limit for syntax and intraline parsing.
pub const DEFAULT_MAX_LINE_CHARACTERS: CharacterCount = CharacterCount::new(2000);

/// Default source-byte limit for each side of a syntax-highlighted diff hunk.
pub const DEFAULT_MAX_SYNTAX_HUNK_BYTES: SyntaxHunkByteLimit = SyntaxHunkByteLimit::new(256 * 1024);

/// Configuration for unified-diff parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    max_line_characters: CharacterCount,
    max_syntax_hunk_bytes: SyntaxHunkByteLimit,
}

impl ParseOptions {
    /// Creates options with the given source-line character limit.
    ///
    /// The leading diff marker is excluded from the count. Lines longer than
    /// this limit remain in the output but skip syntax and intraline parsing.
    #[must_use]
    pub const fn new(max_line_characters: CharacterCount) -> Self {
        Self {
            max_line_characters,
            max_syntax_hunk_bytes: DEFAULT_MAX_SYNTAX_HUNK_BYTES,
        }
    }

    /// Returns the source-line character limit.
    #[must_use]
    pub const fn max_line_characters(self) -> CharacterCount {
        self.max_line_characters
    }

    /// Sets the source-byte limit for each side of a syntax-highlighted hunk.
    #[must_use]
    pub const fn with_max_syntax_hunk_bytes(
        mut self,
        max_syntax_hunk_bytes: SyntaxHunkByteLimit,
    ) -> Self {
        self.max_syntax_hunk_bytes = max_syntax_hunk_bytes;
        self
    }

    /// Returns the source-byte limit for each side of a syntax-highlighted hunk.
    #[must_use]
    pub const fn max_syntax_hunk_bytes(self) -> SyntaxHunkByteLimit {
        self.max_syntax_hunk_bytes
    }
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_LINE_CHARACTERS)
    }
}

/// The semantic role of a parsed diff row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRowKind {
    Meta,
    Hunk,
    Context,
    Added,
    Removed,
}

/// One parsed unified-diff row with owned text and derived data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRow {
    kind: DiffRowKind,
    old_line_number: Option<SourceLineNumber>,
    new_line_number: Option<SourceLineNumber>,
    text: String,
    syntax_tokens: Vec<SyntaxToken>,
    semantic_spans: Vec<SemanticTextSpan>,
    long_line_character_count: Option<CharacterCount>,
}

impl DiffRow {
    #[cfg(feature = "syntax")]
    pub(crate) fn set_syntax_tokens(&mut self, syntax_tokens: Vec<SyntaxToken>) {
        self.syntax_tokens = syntax_tokens;
        self.semantic_spans = semantic_text_spans(self.body(), &self.syntax_tokens, &[]);
    }

    /// Returns the row's semantic role.
    #[must_use]
    pub const fn kind(&self) -> DiffRowKind {
        self.kind
    }

    /// Returns the old-side line number when the row exists on that side.
    #[must_use]
    pub const fn old_line_number(&self) -> Option<SourceLineNumber> {
        self.old_line_number
    }

    /// Returns the new-side line number when the row exists on that side.
    #[must_use]
    pub const fn new_line_number(&self) -> Option<SourceLineNumber> {
        self.new_line_number
    }

    /// Returns the raw diff line, including its leading marker.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the source line with its leading diff marker removed.
    #[must_use]
    pub fn body(&self) -> &str {
        diff_line_body(&self.text)
    }

    /// Returns semantic syntax tokens indexed over [`Self::body`].
    #[must_use]
    pub fn syntax_tokens(&self) -> &[SyntaxToken] {
        &self.syntax_tokens
    }

    /// Returns flat syntax spans over [`Self::body`].
    #[must_use]
    pub fn semantic_spans(&self) -> &[SemanticTextSpan] {
        &self.semantic_spans
    }

    /// Returns the source character count when this row exceeds the parser limit.
    #[must_use]
    pub const fn long_line_character_count(&self) -> Option<CharacterCount> {
        self.long_line_character_count
    }
}

/// One side of a syntax-tokenization diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    Old,
    New,
}

/// A recoverable syntax-tokenization failure.
///
/// The parser retains every diff row. An oversized hunk skips syntax until the
/// next hunk, while an engine failure disables syntax for the rest of the file.
/// Consumers may surface or record these diagnostics at their own boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    side: DiffSide,
    message: String,
}

impl SyntaxDiagnostic {
    #[cfg(feature = "syntax")]
    pub(crate) fn new(side: DiffSide, message: String) -> Self {
        Self { side, message }
    }

    /// Returns the affected side of the diff.
    #[must_use]
    pub const fn side(&self) -> DiffSide {
        self.side
    }

    /// Returns the underlying parser message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// The complete semantic result for one file's unified-diff lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDiff {
    rows: Vec<DiffRow>,
    line_number_digits: LineNumberDigitWidth,
    options: ParseOptions,
    syntax_diagnostics: Vec<SyntaxDiagnostic>,
}

impl ParsedDiff {
    /// Returns parsed rows in source order.
    #[must_use]
    pub fn rows(&self) -> &[DiffRow] {
        &self.rows
    }

    /// Consumes the result and returns its parsed rows.
    #[must_use]
    pub fn into_rows(self) -> Vec<DiffRow> {
        self.rows
    }

    /// Returns the number of decimal digits needed by the largest gutter value.
    #[must_use]
    pub const fn line_number_digits(&self) -> LineNumberDigitWidth {
        self.line_number_digits
    }

    /// Returns the options used to produce this result.
    #[must_use]
    pub const fn options(&self) -> ParseOptions {
        self.options
    }

    /// Returns recoverable syntax-tokenization failures.
    #[must_use]
    pub fn syntax_diagnostics(&self) -> &[SyntaxDiagnostic] {
        &self.syntax_diagnostics
    }

    /// Derives owned rows for a side-by-side diff presentation.
    #[must_use]
    pub fn split_rows(&self) -> Vec<crate::SplitDiffRow> {
        crate::split::split_rows(&self.rows)
    }
}

/// Rows and diagnostics produced from one incremental parser input batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDiffBatch {
    rows: Vec<DiffRow>,
    line_number_digits: LineNumberDigitWidth,
    syntax_diagnostics: Vec<SyntaxDiagnostic>,
}

impl ParsedDiffBatch {
    /// Returns the rows produced by this batch.
    #[must_use]
    pub fn rows(&self) -> &[DiffRow] {
        &self.rows
    }

    /// Consumes the batch and returns its rows.
    #[must_use]
    pub fn into_rows(self) -> Vec<DiffRow> {
        self.rows
    }

    /// Consumes the batch and returns its rows and diagnostics.
    #[must_use]
    pub fn into_parts(self) -> (Vec<DiffRow>, Vec<SyntaxDiagnostic>) {
        (self.rows, self.syntax_diagnostics)
    }

    /// Returns the cumulative gutter width after this batch.
    #[must_use]
    pub const fn line_number_digits(&self) -> LineNumberDigitWidth {
        self.line_number_digits
    }

    /// Returns recoverable failures produced by this batch.
    #[must_use]
    pub fn syntax_diagnostics(&self) -> &[SyntaxDiagnostic] {
        &self.syntax_diagnostics
    }
}

/// Stateful configuration for parsing one file's unified-diff lines.
#[derive(Debug, Clone, Default)]
pub struct DiffParser {
    options: ParseOptions,
    #[cfg(feature = "syntax")]
    syntax: Option<SyntaxLanguage>,
}

impl DiffParser {
    /// Creates a parser with the default line limit and no syntax grammar.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a parser with explicit parsing options.
    #[must_use]
    pub const fn with_options(options: ParseOptions) -> Self {
        Self {
            options,
            #[cfg(feature = "syntax")]
            syntax: None,
        }
    }

    /// Selects an optional syntax grammar for semantic tokenization.
    #[cfg(feature = "syntax")]
    #[must_use]
    pub const fn with_syntax(mut self, syntax: Option<SyntaxLanguage>) -> Self {
        self.syntax = syntax;
        self
    }

    /// Starts an incremental parser with this configuration.
    #[must_use]
    pub fn stream(&self) -> DiffParserStream {
        DiffParserStream {
            options: self.options,
            line_classifier: UnifiedDiffLineClassifier::default(),
            line_numbers: LineNumberState::default(),
            line_number_max: SourceLineNumber::default(),
            #[cfg(feature = "syntax")]
            syntax: self.syntax.map(|language| {
                DiffSyntaxHighlighter::new(language, self.options.max_syntax_hunk_bytes)
            }),
        }
    }

    /// Parses the raw unified-diff lines for one file.
    #[must_use]
    pub fn parse(&self, lines: &[String]) -> ParsedDiff {
        let mut stream = self.stream();
        let batch = stream.push(lines);
        let finished = stream.finish();
        let line_number_digits = finished.line_number_digits();
        let (mut rows, mut syntax_diagnostics) = batch.into_parts();
        let (finished_rows, finished_diagnostics) = finished.into_parts();
        rows.extend(finished_rows);
        syntax_diagnostics.extend(finished_diagnostics);
        ParsedDiff {
            rows,
            line_number_digits,
            options: self.options,
            syntax_diagnostics,
        }
    }
}

/// Incremental semantic parser for one file's unified-diff lines.
pub struct DiffParserStream {
    options: ParseOptions,
    line_classifier: UnifiedDiffLineClassifier,
    line_numbers: LineNumberState,
    line_number_max: SourceLineNumber,
    #[cfg(feature = "syntax")]
    syntax: Option<DiffSyntaxHighlighter>,
}

impl DiffParserStream {
    /// Parses the next source-ordered batch of raw diff lines.
    pub fn push(&mut self, lines: &[String]) -> ParsedDiffBatch {
        let rows = derive_rows(
            lines,
            self.options,
            &mut self.line_classifier,
            &mut self.line_numbers,
        );
        self.line_number_max = self.line_number_max.max(line_number_max(&rows));

        #[cfg(feature = "syntax")]
        let (rows, syntax_diagnostics) = match self.syntax.as_mut() {
            Some(syntax) => {
                let output = syntax.push(rows);
                (output.rows, output.diagnostics)
            }
            None => (rows, Vec::new()),
        };
        #[cfg(not(feature = "syntax"))]
        let syntax_diagnostics = Vec::new();

        ParsedDiffBatch {
            rows,
            line_number_digits: self.line_number_digits(),
            syntax_diagnostics,
        }
    }

    /// Returns the cumulative gutter width after all accepted batches.
    #[must_use]
    pub const fn line_number_digits(&self) -> LineNumberDigitWidth {
        LineNumberDigitWidth::from_source_line_number_max(self.line_number_max)
    }

    /// Returns the options used by this stream.
    #[must_use]
    pub const fn options(&self) -> ParseOptions {
        self.options
    }

    /// Consumes the stream and emits its final buffered syntax hunk.
    #[must_use]
    pub fn finish(self) -> ParsedDiffBatch {
        let line_number_digits = self.line_number_digits();
        #[cfg(feature = "syntax")]
        let (rows, syntax_diagnostics) = self.syntax.map_or_else(
            || (Vec::new(), Vec::new()),
            |syntax| {
                let output = syntax.finish();
                (output.rows, output.diagnostics)
            },
        );
        #[cfg(not(feature = "syntax"))]
        let (rows, syntax_diagnostics) = (Vec::new(), Vec::new());

        ParsedDiffBatch {
            rows,
            line_number_digits,
            syntax_diagnostics,
        }
    }
}

#[derive(Debug, Default)]
struct LineNumberState {
    old: u32,
    new: u32,
}

impl LineNumberState {
    fn advance(
        &mut self,
        line_kind: UnifiedDiffLineKind,
    ) -> (
        DiffRowKind,
        Option<SourceLineNumber>,
        Option<SourceLineNumber>,
    ) {
        match line_kind {
            UnifiedDiffLineKind::Meta => (DiffRowKind::Meta, None, None),
            UnifiedDiffLineKind::Hunk {
                line_number_old,
                line_number_new,
            } => {
                self.old = line_number_old.into_inner();
                self.new = line_number_new.into_inner();
                (DiffRowKind::Hunk, None, None)
            }
            UnifiedDiffLineKind::Added => {
                let new = self.new;
                self.new += 1;
                (DiffRowKind::Added, None, Some(SourceLineNumber::new(new)))
            }
            UnifiedDiffLineKind::Removed => {
                let old = self.old;
                self.old += 1;
                (DiffRowKind::Removed, Some(SourceLineNumber::new(old)), None)
            }
            UnifiedDiffLineKind::Context => {
                let old = self.old;
                let new = self.new;
                self.old += 1;
                self.new += 1;
                (
                    DiffRowKind::Context,
                    Some(SourceLineNumber::new(old)),
                    Some(SourceLineNumber::new(new)),
                )
            }
        }
    }
}

fn derive_rows(
    lines: &[String],
    options: ParseOptions,
    line_classifier: &mut UnifiedDiffLineClassifier,
    line_numbers: &mut LineNumberState,
) -> Vec<DiffRow> {
    let mut rows = Vec::with_capacity(lines.len());

    for raw in lines {
        if raw.is_empty() {
            continue;
        }

        let (kind, old_line_number, new_line_number) =
            line_numbers.advance(line_classifier.classify(raw));
        let semantic_spans = matches!(
            kind,
            DiffRowKind::Context | DiffRowKind::Added | DiffRowKind::Removed
        )
        .then(|| semantic_text_spans(diff_line_body(raw), &[], &[]))
        .unwrap_or_default();
        rows.push(DiffRow {
            kind,
            old_line_number,
            new_line_number,
            text: raw.clone(),
            syntax_tokens: Vec::new(),
            semantic_spans,
            long_line_character_count: long_line_character_count(raw, options.max_line_characters),
        });
    }

    rows
}

fn line_number_max(rows: &[DiffRow]) -> SourceLineNumber {
    rows.iter()
        .flat_map(|row| [row.old_line_number, row.new_line_number])
        .flatten()
        .max()
        .unwrap_or_default()
}

/// Returns a diff source line with its leading marker removed.
#[must_use]
pub fn diff_line_body(raw: &str) -> &str {
    raw.get(1..).unwrap_or("")
}

fn long_line_character_count(
    raw: &str,
    max_line_characters: CharacterCount,
) -> Option<CharacterCount> {
    let marker = usize::from(matches!(raw.as_bytes().first(), Some(b'+' | b'-' | b' ')));
    let count = raw.chars().count().saturating_sub(marker);
    (count > max_line_characters.into_inner()).then_some(CharacterCount::new(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn classifies_rows_and_tracks_gutter_numbers() {
        let parsed = DiffParser::new().parse(&lines(&[
            "index 111..222 100644",
            "@@ -3,2 +7,2 @@",
            " keep",
            "-old",
            "+new",
        ]));
        let rows = parsed.rows();

        assert_eq!(rows[0].kind(), DiffRowKind::Meta);
        assert_eq!(
            (rows[0].old_line_number(), rows[0].new_line_number()),
            (None, None)
        );
        assert_eq!(rows[1].kind(), DiffRowKind::Hunk);
        assert_eq!(rows[1].text(), "@@ -3,2 +7,2 @@");
        assert_eq!(rows[2].kind(), DiffRowKind::Context);
        assert_eq!(
            (rows[2].old_line_number(), rows[2].new_line_number()),
            (
                Some(SourceLineNumber::new(3)),
                Some(SourceLineNumber::new(7))
            )
        );
        assert_eq!(rows[3].kind(), DiffRowKind::Removed);
        assert_eq!(
            (rows[3].old_line_number(), rows[3].new_line_number()),
            (Some(SourceLineNumber::new(4)), None)
        );
        assert_eq!(rows[4].kind(), DiffRowKind::Added);
        assert_eq!(
            (rows[4].old_line_number(), rows[4].new_line_number()),
            (None, Some(SourceLineNumber::new(8)))
        );
        assert_eq!(rows[4].text(), "+new");
        assert_eq!(parsed.line_number_digits().get(), 1);
    }

    #[test]
    fn header_like_hunk_content_keeps_changed_rows_and_gutters() {
        let parsed = DiffParser::new().parse(&lines(&[
            "--- a/a.sql",
            "+++ b/a.sql",
            "@@ -1,2 +1,3 @@",
            "--- old heading",
            "+-- new heading",
            "+++ literal",
            " keep",
        ]));
        let rows = parsed.rows();

        assert_eq!(rows[0].kind(), DiffRowKind::Meta);
        assert_eq!(rows[1].kind(), DiffRowKind::Meta);
        assert_eq!(rows[2].kind(), DiffRowKind::Hunk);
        assert_eq!(rows[3].kind(), DiffRowKind::Removed);
        assert_eq!(rows[4].kind(), DiffRowKind::Added);
        assert_eq!(rows[5].kind(), DiffRowKind::Added);
        assert_eq!(rows[6].kind(), DiffRowKind::Context);
        assert_eq!(rows[6].new_line_number(), Some(SourceLineNumber::new(3)));
    }

    #[test]
    fn malformed_hunk_header_degrades_to_context() {
        let parsed = DiffParser::new().parse(&lines(&["@@ garbage @@"]));
        assert_eq!(parsed.rows()[0].kind(), DiffRowKind::Context);
        assert_eq!(
            parsed.rows()[0].old_line_number(),
            Some(SourceLineNumber::default())
        );
    }

    #[test]
    fn hunk_without_lengths_sets_absolute_starts() {
        let parsed = DiffParser::new().parse(&lines(&["@@ -3 +7 @@", " keep"]));

        assert_eq!(parsed.rows()[0].kind(), DiffRowKind::Hunk);
        assert_eq!(
            parsed.rows()[1].old_line_number(),
            Some(SourceLineNumber::new(3))
        );
        assert_eq!(
            parsed.rows()[1].new_line_number(),
            Some(SourceLineNumber::new(7))
        );
    }

    #[test]
    fn digit_width_tracks_the_largest_source_line_number() {
        let parsed = DiffParser::new().parse(&lines(&["@@ -9999 +10000 @@", " keep"]));

        assert_eq!(parsed.line_number_digits().get(), 5);
    }

    #[test]
    fn file_markers_and_binary_notices_are_metadata() {
        for raw in [
            "--- a/x",
            "+++ b/x",
            "\\ No newline at end of file",
            "Binary files differ",
            "rename from a",
            "new file mode 100644",
            "deleted file mode 100644",
            "old mode 100644",
            "new mode 100755",
            "similarity index 90%",
            "index 111..222",
        ] {
            let parsed = DiffParser::new().parse(&lines(&[raw]));
            assert_eq!(parsed.rows()[0].kind(), DiffRowKind::Meta, "{raw}");
        }
    }

    #[test]
    fn skips_empty_lines_and_strips_source_markers() {
        let parsed = DiffParser::new().parse(&lines(&["@@ -1 +1 @@", "", " x"]));
        assert_eq!(parsed.rows().len(), 2);
        assert_eq!(parsed.rows()[1].body(), "x");
    }

    #[test]
    fn explicit_line_limit_marks_only_overlong_rows() {
        let parsed = DiffParser::with_options(ParseOptions::new(CharacterCount::new(3)))
            .parse(&lines(&["@@ -1 +1 @@", "+abc", "+abcd"]));

        assert_eq!(parsed.rows()[1].long_line_character_count(), None);
        assert_eq!(
            parsed.rows()[2].long_line_character_count(),
            Some(CharacterCount::new(4))
        );
        assert_eq!(
            parsed.options().max_line_characters(),
            CharacterCount::new(3)
        );
    }

    #[test]
    fn line_limit_counts_marker_free_characters() {
        let character_limit = DEFAULT_MAX_LINE_CHARACTERS.into_inner();
        let without_marker = "x".repeat(character_limit + 1);
        let at_limit = format!("+{}", "x".repeat(character_limit));
        let over_limit = format!("+{without_marker}");
        let parsed = DiffParser::new().parse(&[without_marker, at_limit, over_limit]);

        assert_eq!(
            parsed.rows()[0].long_line_character_count(),
            Some(CharacterCount::new(character_limit + 1))
        );
        assert_eq!(parsed.rows()[1].long_line_character_count(), None);
        assert_eq!(
            parsed.rows()[2].long_line_character_count(),
            Some(CharacterCount::new(character_limit + 1))
        );
    }

    #[test]
    fn streaming_pages_match_batch_parsing() {
        let source = lines(&[
            "--- a/file.rs",
            "+++ b/file.rs",
            "@@ -98,3 +998,4 @@",
            " context",
            "-old",
            "+new",
            "+overlong",
            "",
            "\\ No newline at end of file",
            "Binary files differ",
        ]);
        let parser = DiffParser::with_options(ParseOptions::new(CharacterCount::new(5)));
        let expected = parser.parse(&source);
        let expected_split = expected.split_rows();

        for boundary in 0..=source.len() {
            assert_streaming_boundary(&parser, &source, &expected, &expected_split, boundary);
        }
    }

    fn assert_streaming_boundary(
        parser: &DiffParser,
        source: &[String],
        expected: &ParsedDiff,
        expected_split: &[crate::SplitDiffRow],
        boundary: usize,
    ) {
        let mut stream = parser.stream();
        let mut split = crate::SplitDiffStream::new();
        let mut rows = Vec::new();
        let mut split_rows = Vec::new();
        let mut diagnostics = Vec::new();

        for page in [&source[..boundary], &source[boundary..]] {
            let batch = stream.push(page);
            let (batch_rows, batch_diagnostics) = batch.into_parts();
            rows.extend(batch_rows.iter().cloned());
            split_rows.extend(split.push(batch_rows));
            diagnostics.extend(batch_diagnostics);
        }
        split_rows.extend(split.finish());

        assert_eq!(rows, expected.rows(), "row boundary {boundary}");
        assert_eq!(split_rows, expected_split, "split boundary {boundary}");
        assert_eq!(
            diagnostics,
            expected.syntax_diagnostics(),
            "diagnostic boundary {boundary}"
        );
        assert_eq!(
            stream.line_number_digits(),
            expected.line_number_digits(),
            "gutter boundary {boundary}"
        );
        assert_eq!(stream.options(), expected.options());
    }
}
