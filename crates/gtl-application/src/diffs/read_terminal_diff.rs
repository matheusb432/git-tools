use std::path::{Path, PathBuf};

use gtl_models::failure::{ErrorMeta, Failure, ViewerFailure};
use gtl_parser::{
    DiffRowKind, ParseOptions, SyntaxLanguage,
    cancellation::ParseCancellation,
    index::{DiffRowLayout, ParsedDiffWindowRows},
};
use gtl_wire::{
    terminal_diff::{File, Row, RowKind, SnapshotBudget, SnapshotError, TerminalDiff},
    viewer::ViewerCodeSpan,
};

use super::{DiffTarget, diff_computation, fetch_full_context_diff, source_lines::DiffSourceLines};
use crate::{
    ports::{GitClient, RepositoryPreferenceReader},
    viewer::rows::project_syntax_class,
};

pub struct ReadTerminalDiff {
    pub cwd: PathBuf,
    pub target: DiffTarget,
}

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum ReadTerminalDiffError {
    #[error(transparent)]
    #[meta(transparent)]
    Comparison(#[from] crate::projects::comparison::ComparisonError),
    #[error(transparent)]
    #[meta(failure = ViewerFailure::RangeTooLarge)]
    Snapshot(#[from] SnapshotError),
    #[error("repository changed while capturing full context")]
    #[meta(failure = Failure::Changed)]
    Changed,
    #[error(transparent)]
    #[meta(private(Internal))]
    Unexpected(#[from] anyhow::Error),
}

#[cqrsy::query]
pub fn execute(
    request: &ReadTerminalDiff,
    git: &impl GitClient,
    preferences: &impl RepositoryPreferenceReader,
    reviews: &impl crate::ports::DiffReviewReader,
) -> Result<TerminalDiff, ReadTerminalDiffError> {
    let root = git.top_level(&request.cwd)?;
    let filter = preferences.extension_filter(&root)?;
    let computed =
        diff_computation::build(git, &root, &request.target, &filter, preferences, None)?;
    let view = fetch_full_context_diff::load_for_density(
        computed.view,
        gtl_models::viewer::DiffDensity::Full,
        git,
    )?;
    let title = format!(
        "{} · {} · {}",
        view.repo_name, view.branch, computed.summary
    );
    let notes = computed
        .notes
        .into_iter()
        .map(|note| note.text)
        .collect::<Vec<_>>();
    let mut budget = SnapshotBudget::default();
    budget.add_header(&title, &notes)?;
    let references = view
        .files
        .iter()
        .map(|file| super::review::file_review(&view.repo_root, file).reference)
        .collect::<Vec<_>>();
    let reviewed = reviews.reviewed_files(&references)?;
    let mut files = Vec::new();
    for (file, reference) in view.files.into_iter().zip(references) {
        let review = Some(gtl_wire::diff_review::DiffFileReview {
            reviewed: reviewed.contains(&reference),
            reference,
        });
        budget.add_review(review.as_ref())?;
        let path = file.path.to_string_lossy().into_owned();
        budget.add_file(&path)?;
        let compact = project_rows(&file.lines, file.path.as_path(), &mut budget)?;
        let full = file
            .full_lines
            .as_ref()
            .map(|source| project_rows(source, file.path.as_path(), &mut budget))
            .transpose()?
            .unwrap_or_default();
        if !full.is_empty() && !changed_rows(&compact).eq(changed_rows(&full)) {
            return Err(ReadTerminalDiffError::Changed);
        }
        files.push(File {
            review,
            path,
            added: file.added.value(),
            removed: file.removed.value(),
            compact,
            full,
        });
    }
    Ok(TerminalDiff::try_new(title, notes, files)?)
}

fn changed_rows(rows: &[Row]) -> impl Iterator<Item = (&RowKind, &str)> {
    rows.iter()
        .filter(|row| matches!(row.kind, RowKind::Added | RowKind::Removed))
        .map(|row| (&row.kind, row.text.as_str()))
}

fn project_rows(
    source: &DiffSourceLines,
    path: &Path,
    budget: &mut SnapshotBudget,
) -> Result<Vec<Row>, ReadTerminalDiffError> {
    let index = source.index();
    let mut parser = index.window_parser(ParseOptions::default(), SyntaxLanguage::from_path(path));
    let cancellation = ParseCancellation::default();
    let mut rows = Vec::new();
    let length = index.len(DiffRowLayout::Unified);
    for start in (0..length).step_by(128) {
        let parsed = parser
            .parse_range(
                |position| source.line(position),
                start..(start + 128).min(length),
                DiffRowLayout::Unified,
                &cancellation,
            )
            .map_err(anyhow::Error::new)?;
        let ParsedDiffWindowRows::Unified(lines) = parsed.rows else {
            return Err(anyhow::anyhow!("terminal parser returned split rows").into());
        };
        for line in lines {
            let kind = match line.kind() {
                DiffRowKind::Meta => RowKind::Meta,
                DiffRowKind::Hunk => RowKind::Hunk,
                DiffRowKind::Context => RowKind::Context,
                DiffRowKind::Added => RowKind::Added,
                DiffRowKind::Removed => RowKind::Removed,
            };
            let text = match kind {
                RowKind::Meta | RowKind::Hunk => line.text(),
                RowKind::Context | RowKind::Added | RowKind::Removed => line.body(),
            };
            let row = Row {
                kind,
                text: text.to_owned(),
                old_line_number: line
                    .old_line_number()
                    .map(gtl_parser::SourceLineNumber::into_inner),
                new_line_number: line
                    .new_line_number()
                    .map(gtl_parser::SourceLineNumber::into_inner),
                syntax: line
                    .semantic_spans()
                    .iter()
                    .filter(|span| {
                        matches!(kind, RowKind::Context | RowKind::Added | RowKind::Removed)
                            && span.byte_start() < span.byte_end()
                    })
                    .map(|span| ViewerCodeSpan {
                        byte_start: span.byte_start(),
                        byte_end: span.byte_end(),
                        syntax_class: span.syntax_class().map(project_syntax_class),
                        changed: false,
                    })
                    .collect(),
            };
            budget.add_row(&row)?;
            rows.push(row);
        }
    }
    Ok(rows)
}
