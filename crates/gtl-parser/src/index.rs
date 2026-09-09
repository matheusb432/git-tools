use std::ops::Range;

use crate::{
    DiffRow, DiffRowKind, LineNumberDigitWidth, ParseOptions, SourceLineNumber, SplitDiffRow,
    UnifiedDiffLineClassifier, cancellation::ParseCancellation, model::LineNumberState,
};

#[cfg(feature = "syntax")]
mod syntax;

/// The logical row ordering used for a diff document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffRowLayout {
    Unified,
    Split,
}

/// A source entry and its numbered structural role, without rendered text or tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexedLine {
    pub source_index: usize,
    pub kind: DiffRowKind,
    pub old_line_number: Option<SourceLineNumber>,
    pub new_line_number: Option<SourceLineNumber>,
}

/// Source entries contributing to one logical row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexedDiffRow {
    Single(IndexedLine),
    Pair {
        old: Option<IndexedLine>,
        new: Option<IndexedLine>,
    },
}

impl IndexedDiffRow {
    pub fn lines(self) -> impl Iterator<Item = IndexedLine> {
        match self {
            Self::Single(line) => [Some(line), None],
            Self::Pair { old, new } => [old, new],
        }
        .into_iter()
        .flatten()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct IndexedHunk {
    rows: Range<usize>,
    old_bytes: usize,
    new_bytes: usize,
}

/// Immutable source coordinates for direct unified and split row lookup.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DiffIndex {
    unified: Vec<IndexedLine>,
    split: Vec<(Option<usize>, Option<usize>)>,
    hunks: Vec<IndexedHunk>,
    line_number_digits: LineNumberDigitWidth,
}

impl DiffIndex {
    #[must_use]
    pub fn new(lines: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut index = Self::default();
        let mut classifier = UnifiedDiffLineClassifier::default();
        let mut numbers = LineNumberState::default();
        let mut maximum = SourceLineNumber::default();
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for (source_index, text) in lines.into_iter().enumerate() {
            let text = text.as_ref();
            if text.is_empty() {
                continue;
            }
            let (kind, old_line_number, new_line_number) =
                numbers.advance(classifier.classify(text));
            let row_index = index.unified.len();
            index.observe_hunk(row_index, kind, text);
            index.unified.push(IndexedLine {
                source_index,
                kind,
                old_line_number,
                new_line_number,
            });
            for number in [old_line_number, new_line_number].into_iter().flatten() {
                maximum = maximum.max(number);
            }
            match kind {
                DiffRowKind::Removed => removed.push(row_index),
                DiffRowKind::Added => added.push(row_index),
                DiffRowKind::Meta | DiffRowKind::Hunk | DiffRowKind::Context => {
                    flush_pairs(&mut index.split, &mut removed, &mut added);
                    index.split.push((Some(row_index), Some(row_index)));
                }
            }
        }
        flush_pairs(&mut index.split, &mut removed, &mut added);
        if let Some(hunk) = index.hunks.last_mut() {
            hunk.rows.end = index.unified.len();
        }
        index.line_number_digits = LineNumberDigitWidth::from_source_line_number_max(maximum);
        index
    }

    fn observe_hunk(&mut self, row_index: usize, kind: DiffRowKind, text: &str) {
        if kind == DiffRowKind::Hunk {
            if let Some(hunk) = self.hunks.last_mut() {
                hunk.rows.end = row_index;
            }
            self.hunks.push(IndexedHunk {
                rows: row_index..row_index,
                old_bytes: 0,
                new_bytes: 0,
            });
        }
        let Some(hunk) = self.hunks.last_mut() else {
            return;
        };
        let bytes = crate::diff_line_body(text).len().saturating_add(1);
        if matches!(kind, DiffRowKind::Removed | DiffRowKind::Context) {
            hunk.old_bytes = hunk.old_bytes.saturating_add(bytes);
        }
        if matches!(kind, DiffRowKind::Added | DiffRowKind::Context) {
            hunk.new_bytes = hunk.new_bytes.saturating_add(bytes);
        }
    }

    #[must_use]
    pub fn len(&self, layout: DiffRowLayout) -> usize {
        match layout {
            DiffRowLayout::Unified => self.unified.len(),
            DiffRowLayout::Split => self.split.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self, layout: DiffRowLayout) -> bool {
        self.len(layout) == 0
    }

    #[must_use]
    pub const fn line_number_digits(&self) -> LineNumberDigitWidth {
        self.line_number_digits
    }

    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.unified.capacity() * std::mem::size_of::<IndexedLine>()
            + self.split.capacity() * std::mem::size_of::<(Option<usize>, Option<usize>)>()
            + self.hunks.capacity() * std::mem::size_of::<IndexedHunk>()
    }

    #[must_use]
    pub fn row(&self, layout: DiffRowLayout, row_index: usize) -> Option<IndexedDiffRow> {
        match layout {
            DiffRowLayout::Unified => self
                .unified
                .get(row_index)
                .copied()
                .map(IndexedDiffRow::Single),
            DiffRowLayout::Split => {
                let &(old, new) = self.split.get(row_index)?;
                if old == new {
                    return old.map(|index| IndexedDiffRow::Single(self.unified[index]));
                }
                Some(IndexedDiffRow::Pair {
                    old: old.map(|index| self.unified[index]),
                    new: new.map(|index| self.unified[index]),
                })
            }
        }
    }

    /// Parses only the requested rows. Source indices refer to the entries used to build this
    /// index.
    pub fn parse_range<'source>(
        &self,
        source: impl Fn(usize) -> &'source str,
        range: Range<usize>,
        layout: DiffRowLayout,
        options: ParseOptions,
        #[cfg(feature = "syntax")] language: Option<crate::SyntaxLanguage>,
        cancellation: &ParseCancellation,
    ) -> Result<ParsedDiffWindow, DiffWindowError> {
        self.window_parser(
            options,
            #[cfg(feature = "syntax")]
            language,
        )
        .parse_range(source, range, layout, cancellation)
    }

    /// Reuses the last bounded syntax hunk while traversing this immutable source.
    #[must_use]
    pub fn window_parser(
        &self,
        options: ParseOptions,
        #[cfg(feature = "syntax")] language: Option<crate::SyntaxLanguage>,
    ) -> DiffWindowParser<'_> {
        DiffWindowParser {
            index: self,
            options,
            #[cfg(feature = "syntax")]
            syntax: language.map(syntax::WindowHighlighter::new),
        }
    }
}

/// Window projection state for one indexed source. Retains at most one syntax hunk.
pub struct DiffWindowParser<'index> {
    index: &'index DiffIndex,
    options: ParseOptions,
    #[cfg(feature = "syntax")]
    syntax: Option<syntax::WindowHighlighter>,
}

impl DiffWindowParser<'_> {
    /// The source callback must return the same immutable entries used to build the index.
    pub fn parse_range<'source>(
        &mut self,
        source: impl Fn(usize) -> &'source str,
        range: Range<usize>,
        layout: DiffRowLayout,
        cancellation: &ParseCancellation,
    ) -> Result<ParsedDiffWindow, DiffWindowError> {
        check_cancelled(cancellation)?;
        if range.start > range.end || range.end > self.index.len(layout) {
            return Err(DiffWindowError::InvalidRange);
        }
        let mut needed = range
            .clone()
            .filter_map(|row| self.index.row(layout, row))
            .flat_map(IndexedDiffRow::lines)
            .collect::<Vec<_>>();
        needed.sort_unstable_by_key(|line| line.source_index);
        needed.dedup_by_key(|line| line.source_index);
        let mut parsed = Vec::with_capacity(needed.len());
        for line in &needed {
            check_cancelled(cancellation)?;
            parsed.push(DiffRow::indexed(
                source(line.source_index),
                line.kind,
                line.old_line_number,
                line.new_line_number,
                self.options,
            ));
        }
        check_cancelled(cancellation)?;
        #[cfg(feature = "syntax")]
        let diagnostics = match &mut self.syntax {
            Some(syntax) => syntax.highlight(
                self.index,
                &source,
                &needed,
                &mut parsed,
                self.options,
                cancellation,
            )?,
            None => Vec::new(),
        };
        #[cfg(not(feature = "syntax"))]
        let diagnostics = Vec::new();
        let rows = match layout {
            DiffRowLayout::Unified => ParsedDiffWindowRows::Unified(parsed),
            DiffRowLayout::Split => {
                let mut output = Vec::with_capacity(range.len());
                for row in range.filter_map(|row| self.index.row(layout, row)) {
                    check_cancelled(cancellation)?;
                    output.push(project_split(row, &needed, &parsed));
                }
                ParsedDiffWindowRows::Split(output)
            }
        };
        Ok(ParsedDiffWindow { rows, diagnostics })
    }
}

fn flush_pairs(
    output: &mut Vec<(Option<usize>, Option<usize>)>,
    removed: &mut Vec<usize>,
    added: &mut Vec<usize>,
) {
    output.extend(
        (0..removed.len().max(added.len()))
            .map(|index| (removed.get(index).copied(), added.get(index).copied())),
    );
    removed.clear();
    added.clear();
}

fn project_split(row: IndexedDiffRow, needed: &[IndexedLine], parsed: &[DiffRow]) -> SplitDiffRow {
    let get = |line: IndexedLine| {
        let index = needed.partition_point(|candidate| candidate.source_index < line.source_index);
        &parsed[index]
    };
    match row {
        IndexedDiffRow::Single(line) => crate::split::single_row(get(line)),
        IndexedDiffRow::Pair { old, new } => crate::split::paired_row(old.map(get), new.map(get)),
    }
}

/// Semantic rows for a bounded document window.
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedDiffWindow {
    pub rows: ParsedDiffWindowRows,
    pub diagnostics: Vec<crate::SyntaxDiagnostic>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParsedDiffWindowRows {
    Unified(Vec<DiffRow>),
    Split(Vec<SplitDiffRow>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffWindowError {
    InvalidRange,
    Cancelled,
}

impl std::fmt::Display for DiffWindowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRange => "diff row range is invalid",
            Self::Cancelled => "diff row parsing was cancelled",
        })
    }
}

impl std::error::Error for DiffWindowError {}

fn check_cancelled(cancellation: &ParseCancellation) -> Result<(), DiffWindowError> {
    if cancellation.is_cancelled() {
        Err(DiffWindowError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
