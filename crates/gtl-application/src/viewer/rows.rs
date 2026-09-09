//! Pure projection from unified-diff source lines into viewer row contracts.

use gtl_models::paths::RepositoryRelativePath;
use gtl_parser::{
    CharacterCount, DiffParser, DiffParserStream, DiffRow, DiffRowKind, DiffSide,
    SemanticTextChange, SemanticTextSpan, SourceLineNumber, SplitDiffCell, SplitDiffRow,
    SplitDiffStream, SyntaxDiagnostic, SyntaxLanguage, SyntaxTokenClass, diff_line_body,
};
use gtl_wire::viewer::{
    VIEWER_ROW_BATCH_MAX_ROWS, ViewerCodeLine, ViewerCodeSpan, ViewerDiffLayout, ViewerFileRows,
    ViewerRows, ViewerSplitCell, ViewerSplitRow, ViewerSyntaxClass, ViewerUnifiedRow,
    ViewerUnifiedSourceRow,
};

/// Cancellation shared by a viewer request and its CPU worker.
#[derive(Clone, Debug, Default)]
pub struct ViewerWorkCancellation {
    pub(super) parser: gtl_parser::cancellation::ParseCancellation,
}

impl ViewerWorkCancellation {
    pub fn cancel(&self) {
        self.parser.cancel();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.parser.is_cancelled()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ViewerRowWindowError {
    #[error("viewer row range is invalid")]
    InvalidRange,
    #[error("viewer row work was cancelled")]
    Cancelled,
}

pub(crate) const fn indexed_layout(layout: ViewerDiffLayout) -> gtl_parser::index::DiffRowLayout {
    match layout {
        ViewerDiffLayout::Unified => gtl_parser::index::DiffRowLayout::Unified,
        ViewerDiffLayout::Split => gtl_parser::index::DiffRowLayout::Split,
    }
}

#[must_use]
pub fn viewer_file_row_count(
    source: &crate::diffs::source_lines::DiffSourceLines,
    layout: ViewerDiffLayout,
) -> usize {
    source.index().len(indexed_layout(layout))
}

/// Projects adjacent row windows while retaining one bounded syntax hunk.
pub struct ViewerRowWindowParser<'source> {
    source: &'source crate::diffs::source_lines::DiffSourceLines,
    layout: ViewerDiffLayout,
    parser: gtl_parser::index::DiffWindowParser<'source>,
}

impl<'source> ViewerRowWindowParser<'source> {
    #[must_use]
    pub fn new(
        path: &RepositoryRelativePath,
        source: &'source crate::diffs::source_lines::DiffSourceLines,
        layout: ViewerDiffLayout,
    ) -> Self {
        Self {
            source,
            layout,
            parser: source.index().window_parser(
                gtl_parser::ParseOptions::default(),
                SyntaxLanguage::from_path(path.as_path()),
            ),
        }
    }

    pub fn parse(
        &mut self,
        range: std::ops::Range<usize>,
        cancellation: &ViewerWorkCancellation,
    ) -> Result<ViewerRowBatch, ViewerRowWindowError> {
        let parsed = self
            .parser
            .parse_range(
                |line| self.source.line(line),
                range,
                indexed_layout(self.layout),
                &cancellation.parser,
            )
            .map_err(|error| match error {
                gtl_parser::index::DiffWindowError::InvalidRange => {
                    ViewerRowWindowError::InvalidRange
                }
                gtl_parser::index::DiffWindowError::Cancelled => ViewerRowWindowError::Cancelled,
            })?;
        let rows = match parsed.rows {
            gtl_parser::index::ParsedDiffWindowRows::Unified(rows) => {
                ViewerRows::Unified(rows.iter().map(project_unified_row).collect())
            }
            gtl_parser::index::ParsedDiffWindowRows::Split(rows) => {
                ViewerRows::Split(rows.iter().map(project_split_row).collect())
            }
        };
        Ok(ViewerRowBatch {
            rows,
            line_number_digits: self.source.index().line_number_digits().get(),
            diagnostics: project_diagnostics(parsed.diagnostics),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerSyntaxSide {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerSyntaxDiagnostic {
    pub side: ViewerSyntaxSide,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerRowBatch {
    pub rows: ViewerRows,
    pub line_number_digits: u32,
    pub diagnostics: Vec<ViewerSyntaxDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedViewerFile {
    pub file: ViewerFileRows,
    pub diagnostics: Vec<ViewerSyntaxDiagnostic>,
}

pub struct ViewerFileRowParser {
    parser: DiffParserStream,
    split: Option<SplitDiffStream>,
}

impl ViewerFileRowParser {
    #[must_use]
    pub fn new(layout: ViewerDiffLayout, path: &RepositoryRelativePath) -> Self {
        let syntax = SyntaxLanguage::from_path(path.as_path());
        Self {
            parser: DiffParser::new().with_syntax(syntax).stream(),
            split: (layout == ViewerDiffLayout::Split).then(SplitDiffStream::new),
        }
    }

    #[must_use]
    pub fn push(&mut self, lines: impl IntoIterator<Item = impl AsRef<str>>) -> ViewerRowBatch {
        let parsed = self.parser.push(lines);
        let line_number_digits = parsed.line_number_digits().get();
        let (rows, diagnostics) = parsed.into_parts();
        ViewerRowBatch {
            rows: project_rows(&mut self.split, rows),
            line_number_digits,
            diagnostics: project_diagnostics(diagnostics),
        }
    }

    #[must_use]
    pub fn finish(mut self) -> ViewerRowBatch {
        let parsed = self.parser.finish();
        let line_number_digits = parsed.line_number_digits().get();
        let (rows, diagnostics) = parsed.into_parts();
        let mut rows = project_rows(&mut self.split, rows);
        if let Some(split) = self.split {
            append_rows(
                &mut rows,
                ViewerRows::Split(split.finish().iter().map(project_split_row).collect()),
            );
        }
        ViewerRowBatch {
            rows,
            line_number_digits,
            diagnostics: project_diagnostics(diagnostics),
        }
    }
}

fn project_rows(split: &mut Option<SplitDiffStream>, rows: Vec<DiffRow>) -> ViewerRows {
    match split {
        Some(split) => ViewerRows::Split(split.push(rows).iter().map(project_split_row).collect()),
        None => ViewerRows::Unified(rows.iter().map(project_unified_row).collect()),
    }
}

#[must_use]
pub fn parse_viewer_diff_file(
    path: &RepositoryRelativePath,
    lines: impl IntoIterator<Item = impl AsRef<str>>,
    layout: ViewerDiffLayout,
) -> ParsedViewerFile {
    let mut parser = ViewerFileRowParser::new(layout, path);
    let mut rows = empty_rows(layout);
    let mut diagnostics = Vec::new();
    let mut line_number_digits = 1;
    let mut lines = lines.into_iter().peekable();
    while lines.peek().is_some() {
        let batch = parser.push(lines.by_ref().take(VIEWER_ROW_BATCH_MAX_ROWS));
        append_rows(&mut rows, batch.rows);
        line_number_digits = line_number_digits.max(batch.line_number_digits);
        diagnostics.extend(batch.diagnostics);
    }
    let batch = parser.finish();
    append_rows(&mut rows, batch.rows);
    line_number_digits = line_number_digits.max(batch.line_number_digits);
    diagnostics.extend(batch.diagnostics);
    ParsedViewerFile {
        file: ViewerFileRows {
            rows,
            line_number_digits,
        },
        diagnostics,
    }
}

fn empty_rows(layout: ViewerDiffLayout) -> ViewerRows {
    match layout {
        ViewerDiffLayout::Unified => ViewerRows::Unified(Vec::new()),
        ViewerDiffLayout::Split => ViewerRows::Split(Vec::new()),
    }
}

fn append_rows(current: &mut ViewerRows, next: ViewerRows) {
    match (current, next) {
        (ViewerRows::Unified(current), ViewerRows::Unified(mut next)) => {
            current.append(&mut next);
        }
        (ViewerRows::Split(current), ViewerRows::Split(mut next)) => {
            current.append(&mut next);
        }
        (ViewerRows::Unified(_), ViewerRows::Split(_))
        | (ViewerRows::Split(_), ViewerRows::Unified(_)) => {}
    }
}

fn project_unified_row(row: &DiffRow) -> ViewerUnifiedRow {
    match row.kind() {
        DiffRowKind::Meta => ViewerUnifiedRow::Meta(row.text().to_owned()),
        DiffRowKind::Hunk => ViewerUnifiedRow::Hunk(row.text().to_owned()),
        DiffRowKind::Context => ViewerUnifiedRow::Context(project_source_row(row)),
        DiffRowKind::Added => ViewerUnifiedRow::Added(project_source_row(row)),
        DiffRowKind::Removed => ViewerUnifiedRow::Removed(project_source_row(row)),
    }
}

fn project_source_row(row: &DiffRow) -> ViewerUnifiedSourceRow {
    ViewerUnifiedSourceRow {
        old_line_number: row.old_line_number().map(SourceLineNumber::into_inner),
        new_line_number: row.new_line_number().map(SourceLineNumber::into_inner),
        code: project_code_line(
            row.body(),
            row.semantic_spans(),
            row.long_line_character_count()
                .map(CharacterCount::into_inner),
        ),
    }
}

fn project_split_row(row: &SplitDiffRow) -> ViewerSplitRow {
    match row {
        SplitDiffRow::Meta { text } => ViewerSplitRow::Meta(text.clone()),
        SplitDiffRow::Hunk { text } => ViewerSplitRow::Hunk(text.clone()),
        SplitDiffRow::Context {
            old_line_number,
            new_line_number,
            text,
            semantic_spans,
            long_line_character_count,
            ..
        } => ViewerSplitRow::Context {
            old_line_number: old_line_number.into_inner(),
            new_line_number: new_line_number.into_inner(),
            code: project_code_line(
                diff_line_body(text),
                semantic_spans,
                long_line_character_count.map(CharacterCount::into_inner),
            ),
        },
        SplitDiffRow::Pair { old, new } => ViewerSplitRow::Pair {
            old: old.as_ref().map(project_split_cell),
            new: new.as_ref().map(project_split_cell),
        },
    }
}

fn project_split_cell(cell: &SplitDiffCell) -> ViewerSplitCell {
    ViewerSplitCell {
        line_number: cell.line_number().into_inner(),
        code: project_code_line(
            cell.body(),
            cell.semantic_spans(),
            cell.long_line_character_count()
                .map(CharacterCount::into_inner),
        ),
    }
}

fn project_code_line(
    text: &str,
    spans: &[SemanticTextSpan],
    long_line_character_count: Option<usize>,
) -> ViewerCodeLine {
    ViewerCodeLine {
        text: text.to_owned(),
        spans: spans
            .iter()
            .map(|span| ViewerCodeSpan {
                byte_start: span.byte_start(),
                byte_end: span.byte_end(),
                syntax_class: span.syntax_class().map(project_syntax_class),
                changed: span.change() == SemanticTextChange::Changed,
            })
            .collect(),
        long_line_character_count,
    }
}

const fn project_syntax_class(class: SyntaxTokenClass) -> ViewerSyntaxClass {
    match class {
        SyntaxTokenClass::Keyword => ViewerSyntaxClass::Keyword,
        SyntaxTokenClass::String => ViewerSyntaxClass::String,
        SyntaxTokenClass::Comment => ViewerSyntaxClass::Comment,
        SyntaxTokenClass::Type => ViewerSyntaxClass::Type,
        SyntaxTokenClass::Function => ViewerSyntaxClass::Function,
        SyntaxTokenClass::Number => ViewerSyntaxClass::Number,
        SyntaxTokenClass::Constant => ViewerSyntaxClass::Constant,
        SyntaxTokenClass::Operator => ViewerSyntaxClass::Operator,
        SyntaxTokenClass::Tag => ViewerSyntaxClass::Tag,
        SyntaxTokenClass::Variable => ViewerSyntaxClass::Variable,
    }
}

fn project_diagnostics(diagnostics: Vec<SyntaxDiagnostic>) -> Vec<ViewerSyntaxDiagnostic> {
    diagnostics
        .into_iter()
        .map(|diagnostic| ViewerSyntaxDiagnostic {
            side: match diagnostic.side() {
                DiffSide::Old => ViewerSyntaxSide::Old,
                DiffSide::New => ViewerSyntaxSide::New,
            },
            message: diagnostic.message().to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::{ViewerRows, ViewerSyntaxClass, ViewerUnifiedRow};

    use super::*;
    use crate::utils::repository_relative_path;

    #[test]
    fn indexed_windows_project_the_same_rows_as_complete_artifacts() {
        let path = repository_relative_path("src/example.rs");
        let source = [
            "--- a/example.rs",
            "+++ b/example.rs",
            "@@ -40,2 +50,2 @@",
            "-let old = 1;",
            "+let new = 2;",
            " context",
            "",
        ]
        .into_iter()
        .collect::<crate::diffs::source_lines::DiffSourceLines>();
        for layout in [ViewerDiffLayout::Unified, ViewerDiffLayout::Split] {
            let expected = parse_viewer_diff_file(&path, &source, layout).file;
            let mut rows = empty_rows(layout);
            let mut parser = ViewerRowWindowParser::new(&path, &source, layout);
            for start in 0..viewer_file_row_count(&source, layout) {
                let window = parser
                    .parse(start..start + 1, &ViewerWorkCancellation::default())
                    .unwrap();
                assert_eq!(window.line_number_digits, expected.line_number_digits);
                append_rows(&mut rows, window.rows);
            }
            assert_eq!(rows, expected.rows);
        }
    }

    #[test]
    fn projects_syntax_and_line_numbers_for_supported_files() {
        let parsed = parse_viewer_diff_file(
            &repository_relative_path("src/example.rs"),
            &[
                "@@ -1 +1 @@".to_owned(),
                "-let old_value = 1;".to_owned(),
                "+let new_value = 2;".to_owned(),
            ],
            ViewerDiffLayout::Unified,
        );

        let rows = match parsed.file.rows {
            ViewerRows::Unified(rows) => Some(rows),
            ViewerRows::Split(_) => None,
        }
        .unwrap();
        assert!(parsed.file.line_number_digits >= 1);
        assert!(rows.iter().any(|row| {
            match row {
                ViewerUnifiedRow::Context(row)
                | ViewerUnifiedRow::Added(row)
                | ViewerUnifiedRow::Removed(row) => row
                    .code
                    .spans
                    .iter()
                    .any(|span| span.syntax_class == Some(ViewerSyntaxClass::Keyword)),
                ViewerUnifiedRow::Meta(_) | ViewerUnifiedRow::Hunk(_) => false,
            }
        }));
    }

    #[test]
    fn split_projection_finishes_unpaired_rows() {
        let parsed = parse_viewer_diff_file(
            &repository_relative_path("src/example.rs"),
            &["@@ -1 +1 @@".to_owned(), "-removed".to_owned()],
            ViewerDiffLayout::Split,
        );

        let rows = match parsed.file.rows {
            ViewerRows::Split(rows) => Some(rows),
            ViewerRows::Unified(_) => None,
        }
        .unwrap();
        assert!(matches!(
            rows.last(),
            Some(ViewerSplitRow::Pair {
                old: Some(_),
                new: None
            })
        ));
    }
}
