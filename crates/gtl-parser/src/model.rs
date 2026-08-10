#[cfg(feature = "syntax")]
use crate::{SyntaxDefinition, highlight::DiffSyntaxHighlighter};
use crate::{SyntaxToken, UnifiedDiffLineClassifier, UnifiedDiffLineKind};

/// Default source-line character limit for syntax and intraline parsing.
pub const DEFAULT_MAX_LINE_CHARACTERS: usize = 2000;

/// Configuration for unified-diff parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOptions {
    max_line_characters: usize,
}

impl ParseOptions {
    /// Creates options with the given source-line character limit.
    ///
    /// The leading diff marker is excluded from the count. Lines longer than
    /// this limit remain in the output but skip syntax and intraline parsing.
    pub const fn new(max_line_characters: usize) -> Self {
        Self {
            max_line_characters,
        }
    }

    /// Returns the source-line character limit.
    pub const fn max_line_characters(self) -> usize {
        self.max_line_characters
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
    old_line_number: Option<u32>,
    new_line_number: Option<u32>,
    text: String,
    syntax_tokens: Vec<SyntaxToken>,
    long_line_character_count: Option<usize>,
}

impl DiffRow {
    #[cfg(feature = "syntax")]
    pub(crate) fn set_syntax_tokens(&mut self, syntax_tokens: Vec<SyntaxToken>) {
        self.syntax_tokens = syntax_tokens;
    }

    /// Returns the row's semantic role.
    pub const fn kind(&self) -> DiffRowKind {
        self.kind
    }

    /// Returns the old-side line number when the row exists on that side.
    pub const fn old_line_number(&self) -> Option<u32> {
        self.old_line_number
    }

    /// Returns the new-side line number when the row exists on that side.
    pub const fn new_line_number(&self) -> Option<u32> {
        self.new_line_number
    }

    /// Returns the raw diff line, including its leading marker.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the source line with its leading diff marker removed.
    pub fn body(&self) -> &str {
        diff_line_body(&self.text)
    }

    /// Returns semantic syntax tokens indexed over [`Self::body`].
    pub fn syntax_tokens(&self) -> &[SyntaxToken] {
        &self.syntax_tokens
    }

    /// Returns the source character count when this row exceeds the parser limit.
    pub const fn long_line_character_count(&self) -> Option<usize> {
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
/// The parser retains the diff row and disables syntax parsing for the affected
/// side. Consumers may surface or record these diagnostics at their own boundary.
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
    pub const fn side(&self) -> DiffSide {
        self.side
    }

    /// Returns the underlying parser message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// The complete semantic result for one file's unified-diff lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDiff {
    rows: Vec<DiffRow>,
    line_number_digits: u32,
    options: ParseOptions,
    syntax_diagnostics: Vec<SyntaxDiagnostic>,
}

impl ParsedDiff {
    /// Returns parsed rows in source order.
    pub fn rows(&self) -> &[DiffRow] {
        &self.rows
    }

    /// Consumes the result and returns its parsed rows.
    pub fn into_rows(self) -> Vec<DiffRow> {
        self.rows
    }

    /// Returns the number of decimal digits needed by the largest gutter value.
    pub const fn line_number_digits(&self) -> u32 {
        self.line_number_digits
    }

    /// Returns the options used to produce this result.
    pub const fn options(&self) -> ParseOptions {
        self.options
    }

    /// Returns recoverable syntax-tokenization failures.
    pub fn syntax_diagnostics(&self) -> &[SyntaxDiagnostic] {
        &self.syntax_diagnostics
    }

    /// Derives owned rows for a side-by-side diff presentation.
    pub fn split_rows(&self) -> Vec<crate::SplitDiffRow> {
        crate::split::split_rows(&self.rows)
    }
}

/// Rows and diagnostics produced from one incremental parser input batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDiffBatch {
    rows: Vec<DiffRow>,
    line_number_digits: u32,
    syntax_diagnostics: Vec<SyntaxDiagnostic>,
}

impl ParsedDiffBatch {
    /// Returns the rows produced by this batch.
    pub fn rows(&self) -> &[DiffRow] {
        &self.rows
    }

    /// Consumes the batch and returns its rows.
    pub fn into_rows(self) -> Vec<DiffRow> {
        self.rows
    }

    /// Consumes the batch and returns its rows and diagnostics.
    pub fn into_parts(self) -> (Vec<DiffRow>, Vec<SyntaxDiagnostic>) {
        (self.rows, self.syntax_diagnostics)
    }

    /// Returns the cumulative gutter width after this batch.
    pub const fn line_number_digits(&self) -> u32 {
        self.line_number_digits
    }

    /// Returns recoverable failures produced by this batch.
    pub fn syntax_diagnostics(&self) -> &[SyntaxDiagnostic] {
        &self.syntax_diagnostics
    }
}

/// Stateful configuration for parsing one file's unified-diff lines.
#[derive(Debug, Clone, Default)]
pub struct DiffParser {
    options: ParseOptions,
    #[cfg(feature = "syntax")]
    syntax: Option<SyntaxDefinition>,
}

impl DiffParser {
    /// Creates a parser with the default line limit and no syntax grammar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a parser with explicit parsing options.
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
    pub fn with_syntax(mut self, syntax: Option<SyntaxDefinition>) -> Self {
        self.syntax = syntax;
        self
    }

    /// Starts an incremental parser with this configuration.
    pub fn stream(&self) -> DiffParserStream {
        DiffParserStream {
            options: self.options,
            line_classifier: UnifiedDiffLineClassifier::default(),
            line_numbers: LineNumberState::default(),
            line_number_max: 0,
            #[cfg(feature = "syntax")]
            syntax: self.syntax.as_ref().map(DiffSyntaxHighlighter::new),
        }
    }

    /// Parses the raw unified-diff lines for one file.
    pub fn parse(&self, lines: &[String]) -> ParsedDiff {
        let mut stream = self.stream();
        let batch = stream.push(lines);
        ParsedDiff {
            rows: batch.rows,
            line_number_digits: batch.line_number_digits,
            options: self.options,
            syntax_diagnostics: batch.syntax_diagnostics,
        }
    }
}

/// Incremental semantic parser for one file's unified-diff lines.
pub struct DiffParserStream {
    options: ParseOptions,
    line_classifier: UnifiedDiffLineClassifier,
    line_numbers: LineNumberState,
    line_number_max: u32,
    #[cfg(feature = "syntax")]
    syntax: Option<DiffSyntaxHighlighter>,
}

impl DiffParserStream {
    /// Parses the next source-ordered batch of raw diff lines.
    pub fn push(&mut self, lines: &[String]) -> ParsedDiffBatch {
        #[allow(
            unused_mut,
            reason = "syntax-enabled builds attach tokens after row derivation"
        )]
        let mut rows = derive_rows(
            lines,
            self.options,
            &mut self.line_classifier,
            &mut self.line_numbers,
        );
        self.line_number_max = self.line_number_max.max(line_number_max(&rows));

        #[allow(unused_mut)]
        let mut syntax_diagnostics = Vec::new();
        #[cfg(feature = "syntax")]
        if let Some(syntax) = &mut self.syntax {
            syntax_diagnostics = syntax.attach(&mut rows);
        }

        ParsedDiffBatch {
            rows,
            line_number_digits: self.line_number_digits(),
            syntax_diagnostics,
        }
    }

    /// Returns the cumulative gutter width after all accepted batches.
    pub const fn line_number_digits(&self) -> u32 {
        line_number_digits(self.line_number_max)
    }

    /// Returns the options used by this stream.
    pub const fn options(&self) -> ParseOptions {
        self.options
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
    ) -> (DiffRowKind, Option<u32>, Option<u32>) {
        match line_kind {
            UnifiedDiffLineKind::Meta => (DiffRowKind::Meta, None, None),
            UnifiedDiffLineKind::Hunk {
                line_number_old,
                line_number_new,
            } => {
                self.old = line_number_old;
                self.new = line_number_new;
                (DiffRowKind::Hunk, None, None)
            }
            UnifiedDiffLineKind::Added => {
                let new = self.new;
                self.new += 1;
                (DiffRowKind::Added, None, Some(new))
            }
            UnifiedDiffLineKind::Removed => {
                let old = self.old;
                self.old += 1;
                (DiffRowKind::Removed, Some(old), None)
            }
            UnifiedDiffLineKind::Context => {
                let old = self.old;
                let new = self.new;
                self.old += 1;
                self.new += 1;
                (DiffRowKind::Context, Some(old), Some(new))
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
        rows.push(DiffRow {
            kind,
            old_line_number,
            new_line_number,
            text: raw.clone(),
            syntax_tokens: Vec::new(),
            long_line_character_count: long_line_character_count(raw, options.max_line_characters),
        });
    }

    rows
}

fn line_number_max(rows: &[DiffRow]) -> u32 {
    rows.iter()
        .flat_map(|row| [row.old_line_number, row.new_line_number])
        .flatten()
        .max()
        .unwrap_or(0)
}

const fn line_number_digits(line_number_max: u32) -> u32 {
    match line_number_max.checked_ilog10() {
        Some(digits) => digits + 1,
        None => 1,
    }
}

/// Returns a diff source line with its leading marker removed.
pub fn diff_line_body(raw: &str) -> &str {
    raw.get(1..).unwrap_or("")
}

fn long_line_character_count(raw: &str, max_line_characters: usize) -> Option<usize> {
    let marker = usize::from(matches!(raw.as_bytes().first(), Some(b'+' | b'-' | b' ')));
    let count = raw.chars().count().saturating_sub(marker);
    (count > max_line_characters).then_some(count)
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
            (Some(3), Some(7))
        );
        assert_eq!(rows[3].kind(), DiffRowKind::Removed);
        assert_eq!(
            (rows[3].old_line_number(), rows[3].new_line_number()),
            (Some(4), None)
        );
        assert_eq!(rows[4].kind(), DiffRowKind::Added);
        assert_eq!(
            (rows[4].old_line_number(), rows[4].new_line_number()),
            (None, Some(8))
        );
        assert_eq!(rows[4].text(), "+new");
        assert_eq!(parsed.line_number_digits(), 1);
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
        assert_eq!(rows[6].new_line_number(), Some(3));
    }

    #[test]
    fn malformed_hunk_header_degrades_to_context() {
        let parsed = DiffParser::new().parse(&lines(&["@@ garbage @@"]));
        assert_eq!(parsed.rows()[0].kind(), DiffRowKind::Context);
        assert_eq!(parsed.rows()[0].old_line_number(), Some(0));
    }

    #[test]
    fn hunk_without_lengths_sets_absolute_starts() {
        let parsed = DiffParser::new().parse(&lines(&["@@ -3 +7 @@", " keep"]));

        assert_eq!(parsed.rows()[0].kind(), DiffRowKind::Hunk);
        assert_eq!(parsed.rows()[1].old_line_number(), Some(3));
        assert_eq!(parsed.rows()[1].new_line_number(), Some(7));
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
        let parsed = DiffParser::with_options(ParseOptions::new(3)).parse(&lines(&[
            "@@ -1 +1 @@",
            "+abc",
            "+abcd",
        ]));

        assert_eq!(parsed.rows()[1].long_line_character_count(), None);
        assert_eq!(parsed.rows()[2].long_line_character_count(), Some(4));
        assert_eq!(parsed.options().max_line_characters(), 3);
    }

    #[test]
    fn line_limit_counts_marker_free_characters() {
        let without_marker = "x".repeat(DEFAULT_MAX_LINE_CHARACTERS + 1);
        let at_limit = format!("+{}", "x".repeat(DEFAULT_MAX_LINE_CHARACTERS));
        let over_limit = format!("+{without_marker}");
        let parsed = DiffParser::new().parse(&[without_marker, at_limit, over_limit]);

        assert_eq!(
            parsed.rows()[0].long_line_character_count(),
            Some(DEFAULT_MAX_LINE_CHARACTERS + 1)
        );
        assert_eq!(parsed.rows()[1].long_line_character_count(), None);
        assert_eq!(
            parsed.rows()[2].long_line_character_count(),
            Some(DEFAULT_MAX_LINE_CHARACTERS + 1)
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
        let parser = DiffParser::with_options(ParseOptions::new(5));
        let expected = parser.parse(&source);
        let expected_split = expected.split_rows();

        for boundary in 0..=source.len() {
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
}
