use super::{DiffIndex, DiffWindowError, IndexedLine, check_cancelled};
use crate::{
    DiffRow, DiffRowKind, ParseOptions, SyntaxLanguage,
    cancellation::ParseCancellation,
    highlight::{HunkSource, SourceRowRange, tokens_by_row},
    syntax::{ByteSyntaxToken, SideHighlighter},
};

pub(super) struct WindowHighlighter {
    language: SyntaxLanguage,
    engine: SideHighlighter,
    cached: Option<CachedHunk>,
}

struct CachedHunk {
    index: usize,
    sides: [CachedSide; 2],
}

struct CachedSide {
    source: HunkSource,
    tokens: Vec<ByteSyntaxToken>,
}

impl WindowHighlighter {
    pub(super) fn new(language: SyntaxLanguage) -> Self {
        Self {
            language,
            engine: SideHighlighter::new(),
            cached: None,
        }
    }

    pub(super) fn highlight<'source>(
        &mut self,
        index: &DiffIndex,
        source: &impl Fn(usize) -> &'source str,
        needed: &[IndexedLine],
        parsed: &mut [DiffRow],
        options: ParseOptions,
        cancellation: &ParseCancellation,
    ) -> Result<Vec<crate::SyntaxDiagnostic>, DiffWindowError> {
        let mut hunks = needed
            .iter()
            .filter_map(|line| {
                let row = index
                    .unified
                    .partition_point(|candidate| candidate.source_index < line.source_index);
                index
                    .hunks
                    .partition_point(|hunk| hunk.rows.start <= row)
                    .checked_sub(1)
            })
            .collect::<Vec<_>>();
        hunks.dedup();
        let mut diagnostics = Vec::new();
        for hunk_index in hunks {
            check_cancelled(cancellation)?;
            let hunk = &index.hunks[hunk_index];
            let limit = options.max_syntax_hunk_bytes().into_inner();
            if let Some(diagnostic) = hunk_limit_diagnostic(hunk, limit) {
                self.cached = None;
                diagnostics.push(diagnostic);
                continue;
            }
            match self.highlight_hunk(
                hunk_index,
                &index.unified[hunk.rows.clone()],
                source,
                needed,
                parsed,
                cancellation,
            ) {
                Ok(()) => {}
                Err(HunkHighlightError::Cancelled) => return Err(DiffWindowError::Cancelled),
                Err(HunkHighlightError::Syntax(diagnostic)) => {
                    diagnostics.push(diagnostic);
                    return Ok(diagnostics);
                }
            }
        }
        Ok(diagnostics)
    }

    fn highlight_hunk<'source>(
        &mut self,
        hunk_index: usize,
        lines: &[IndexedLine],
        source: &impl Fn(usize) -> &'source str,
        needed: &[IndexedLine],
        parsed: &mut [DiffRow],
        cancellation: &ParseCancellation,
    ) -> Result<(), HunkHighlightError> {
        if self
            .cached
            .as_ref()
            .is_none_or(|cached| cached.index != hunk_index)
        {
            self.cached = None;
            self.cached =
                Some(self.prepare(hunk_index, lines, source, self.language, cancellation)?);
        }
        if let Some(cached) = &self.cached {
            attach_headers(needed, parsed, lines);
            cached.attach(needed, parsed, cancellation)?;
        }
        Ok(())
    }

    fn prepare<'source>(
        &mut self,
        index: usize,
        lines: &[IndexedLine],
        source: &impl Fn(usize) -> &'source str,
        language: SyntaxLanguage,
        cancellation: &ParseCancellation,
    ) -> Result<CachedHunk, HunkHighlightError> {
        let mut side = |old| {
            let source = reconstruct(lines, source, old, cancellation)
                .map_err(|_| HunkHighlightError::Cancelled)?;
            let tokens = self
                .engine
                .tokens(language, &source.text, Some(cancellation.flag()));
            check_cancelled(cancellation).map_err(|_| HunkHighlightError::Cancelled)?;
            let tokens =
                tokens.map_err(|message| HunkHighlightError::Syntax(diagnostic(old, message)))?;
            Ok::<_, HunkHighlightError>(CachedSide { source, tokens })
        };
        Ok(CachedHunk {
            index,
            sides: [side(true)?, side(false)?],
        })
    }
}

impl CachedHunk {
    fn attach(
        &self,
        needed: &[IndexedLine],
        parsed: &mut [DiffRow],
        cancellation: &ParseCancellation,
    ) -> Result<(), HunkHighlightError> {
        for (old, side) in [true, false].into_iter().zip(&self.sides) {
            attach_tokens(needed, parsed, side)
                .map_err(|message| HunkHighlightError::Syntax(diagnostic(old, message)))?;
            check_cancelled(cancellation).map_err(|_| HunkHighlightError::Cancelled)?;
        }
        Ok(())
    }
}

fn hunk_limit_diagnostic(
    hunk: &super::IndexedHunk,
    limit: usize,
) -> Option<crate::SyntaxDiagnostic> {
    if hunk.old_bytes.max(hunk.new_bytes) <= limit {
        return None;
    }
    let side = if hunk.old_bytes > limit {
        crate::DiffSide::Old
    } else {
        crate::DiffSide::New
    };
    Some(crate::SyntaxDiagnostic::new(
        side,
        format!("syntax hunk {side:?} source exceeds the {limit}-byte limit"),
    ))
}

fn attach_headers(needed: &[IndexedLine], parsed: &mut [DiffRow], lines: &[IndexedLine]) {
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
        return;
    };
    for (line, row) in needed.iter().zip(parsed) {
        if matches!(line.kind, DiffRowKind::Meta | DiffRowKind::Hunk)
            && line.source_index >= first.source_index
            && line.source_index <= last.source_index
        {
            row.set_syntax_tokens(Vec::new());
        }
    }
}

enum HunkHighlightError {
    Cancelled,
    Syntax(crate::SyntaxDiagnostic),
}

fn diagnostic(old: bool, message: String) -> crate::SyntaxDiagnostic {
    crate::SyntaxDiagnostic::new(
        if old {
            crate::DiffSide::Old
        } else {
            crate::DiffSide::New
        },
        message,
    )
}

fn attach_tokens(
    needed: &[IndexedLine],
    parsed: &mut [DiffRow],
    side: &CachedSide,
) -> Result<(), String> {
    let rows = needed
        .iter()
        .zip(parsed.iter())
        .enumerate()
        .filter_map(|(row_index, (line, row))| {
            let source_row = side
                .source
                .rows
                .partition_point(|candidate| candidate.row_index < line.source_index);
            side.source
                .rows
                .get(source_row)
                .filter(|candidate| candidate.row_index == line.source_index)
                .map(|candidate| SourceRowRange {
                    row_index,
                    byte_range: candidate.byte_range.clone(),
                    suppress_tokens: row.long_line_character_count().is_some(),
                })
        })
        .collect::<Vec<_>>();
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return Ok(());
    };
    let start = side
        .tokens
        .partition_point(|token| token.end <= first.byte_range.start);
    let end = side
        .tokens
        .partition_point(|token| token.start < last.byte_range.end);
    let mut tokens = tokens_by_row(
        &side.source.text,
        &rows,
        &side.tokens[start..end],
        parsed.len(),
    )?;
    for source_row in rows {
        parsed[source_row.row_index]
            .set_syntax_tokens(std::mem::take(&mut tokens[source_row.row_index]));
    }
    Ok(())
}

fn reconstruct<'source>(
    lines: &[IndexedLine],
    source: &impl Fn(usize) -> &'source str,
    old: bool,
    cancellation: &ParseCancellation,
) -> Result<HunkSource, DiffWindowError> {
    let mut output = HunkSource::default();
    for line in lines {
        check_cancelled(cancellation)?;
        if !belongs_to_side(line.kind, old) {
            continue;
        }
        let start = output.text.len();
        output
            .text
            .push_str(crate::diff_line_body(source(line.source_index)));
        let end = output.text.len();
        output.text.push('\n');
        if !old || line.kind == DiffRowKind::Removed {
            output.rows.push(SourceRowRange {
                row_index: line.source_index,
                byte_range: start..end,
                suppress_tokens: false,
            });
        }
    }
    Ok(output)
}

const fn belongs_to_side(kind: DiffRowKind, old: bool) -> bool {
    match kind {
        DiffRowKind::Context => true,
        DiffRowKind::Removed => old,
        DiffRowKind::Added => !old,
        DiffRowKind::Meta | DiffRowKind::Hunk => false,
    }
}
