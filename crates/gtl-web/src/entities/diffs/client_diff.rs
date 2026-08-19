use std::{sync::Arc, time::Duration};

use dioxus::prelude::*;
use gtl_parser::{
    DiffParser, DiffRow, LineNumberDigitWidth, SourceLineNumber, SplitDiffRow, SplitDiffStream,
    SyntaxLanguage,
};
use gtl_wire::viewer::{
    LoadViewerDiffLines, VIEWER_DIFF_LINES_PAGE_MAX_BYTES, ViewerDiffCursor, ViewerDiffLayout,
    ViewerDiffLines, ViewerFileSummary, ViewerViewIdentity,
};

use self::source::ClientDiffSourceError;

mod source;

pub(crate) use source::ClientDiffSource;

const CLIENT_LINE_BATCH_SIZE: usize = 64;

/// Orders asynchronous page-loading requests independently from viewer identity generations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ClientDiffRequestTicket(u64);

impl ClientDiffRequestTicket {
    #[must_use]
    const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientDiffRows {
    // TODO: refactor. vec<arc<vec<_>>> , i mean. $_$
    Unified(Vec<Arc<Vec<DiffRow>>>),
    Split(Vec<Arc<Vec<SplitDiffRow>>>),
}

impl ClientDiffRows {
    fn new(layout: ViewerDiffLayout) -> Self {
        match layout {
            ViewerDiffLayout::Unified => Self::Unified(Vec::new()),
            ViewerDiffLayout::Split => Self::Split(Vec::new()),
        }
    }

    fn append_unified(&mut self, rows: Vec<DiffRow>) {
        if let Self::Unified(batches) = self {
            append_bounded_batches(batches, rows);
        }
    }

    fn append_split(&mut self, rows: Vec<SplitDiffRow>) {
        if let Self::Split(batches) = self {
            append_bounded_batches(batches, rows);
        }
    }
}

fn append_bounded_batches<Row>(batches: &mut Vec<Arc<Vec<Row>>>, rows: Vec<Row>) {
    let mut rows = rows.into_iter();
    loop {
        let batch = rows
            .by_ref()
            .take(CLIENT_LINE_BATCH_SIZE)
            .collect::<Vec<_>>();
        if batch.is_empty() {
            return;
        }
        batches.push(Arc::new(batch));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientDiffFileError {
    Source(ClientDiffSourceError),
    InvalidPage,
}

impl ClientDiffFileError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Source(error) => error.message(),
            Self::InvalidPage => {
                "The diff source returned an invalid page. Retry this view to load it again."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientDiffFileState {
    Loading,
    Complete,
    Error(ClientDiffFileError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffFile {
    pub(crate) summary: ViewerFileSummary,
    pub(crate) rows: ClientDiffRows,
    pub(crate) line_number_digits: LineNumberDigitWidth,
    pub(crate) state: ClientDiffFileState,
}

impl ClientDiffFile {
    fn loading(summary: ViewerFileSummary, layout: ViewerDiffLayout) -> Self {
        Self {
            summary,
            rows: ClientDiffRows::new(layout),
            line_number_digits: LineNumberDigitWidth::default(),
            state: ClientDiffFileState::Loading,
        }
    }

    pub(crate) fn copy_code(&self, include_context: bool) -> String {
        let copied = match &self.rows {
            ClientDiffRows::Unified(batches) => {
                copied_unified_rows(batches.iter().flat_map(|batch| batch.iter()))
            }
            ClientDiffRows::Split(batches) => {
                copied_split_rows(batches.iter().flat_map(|batch| batch.iter()))
            }
        };
        let code = copied.lines.join("\n");
        if !include_context || copied.lines.is_empty() {
            return code;
        }

        let line_range = copied.line_range();
        format!(
            "{} * {}{}\n{code}",
            comment_leader(&self.summary.path),
            self.summary.path.display(),
            line_range
                .map(|range| format!(", lines: {range}"))
                .unwrap_or_default(),
        )
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct CopiedRows {
    lines: Vec<String>,
    first_line: Option<SourceLineNumber>,
    last_line: Option<SourceLineNumber>,
}

impl CopiedRows {
    fn push(&mut self, line: String, line_number: Option<SourceLineNumber>) {
        self.lines.push(line);
        if let Some(line_number) = line_number {
            self.first_line.get_or_insert(line_number);
            self.last_line = Some(line_number);
        }
    }

    fn line_range(&self) -> Option<String> {
        match (self.first_line, self.last_line) {
            (Some(first), Some(last)) if first == last => Some(first.to_string()),
            (Some(first), Some(last)) => Some(format!("{first}..{last}")),
            _ => None,
        }
    }
}

fn copied_unified_rows<'rows>(rows: impl Iterator<Item = &'rows DiffRow>) -> CopiedRows {
    let mut copied = CopiedRows::default();
    for row in rows {
        if matches!(
            row.kind(),
            gtl_parser::DiffRowKind::Added | gtl_parser::DiffRowKind::Context
        ) {
            copied.push(row.body().to_owned(), row.new_line_number());
        }
    }
    copied
}

fn copied_split_rows<'rows>(rows: impl Iterator<Item = &'rows SplitDiffRow>) -> CopiedRows {
    let mut copied = CopiedRows::default();
    for row in rows {
        match row {
            SplitDiffRow::Context {
                new_line_number,
                text,
                ..
            } => copied.push(
                gtl_parser::diff_line_body(text).to_owned(),
                Some(*new_line_number),
            ),
            SplitDiffRow::Pair {
                new: Some(cell), ..
            } => copied.push(
                gtl_parser::diff_line_body(cell.text()).to_owned(),
                Some(cell.line_number()),
            ),
            SplitDiffRow::Meta { .. }
            | SplitDiffRow::Hunk { .. }
            | SplitDiffRow::Pair { new: None, .. } => {}
        }
    }
    copied
}

fn comment_leader(path: &gtl_models::paths::RepositoryRelativePath) -> &'static str {
    let extension = path
        .as_path()
        .extension()
        .and_then(|extension| extension.to_str());
    match extension {
        Some(extension) if extension.eq_ignore_ascii_case("sh") => "#",
        _ => "//",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffWorkspace {
    pub(crate) identity: ViewerViewIdentity,
    pub(crate) files: Vec<ClientDiffFile>,
}

impl ClientDiffWorkspace {
    fn loading(identity: ViewerViewIdentity, files: Vec<ViewerFileSummary>) -> Self {
        Self {
            identity,
            files: files
                .into_iter()
                .map(|file| ClientDiffFile::loading(file, identity.render_options.layout))
                .collect(),
        }
    }

    pub(crate) fn is_loading(&self) -> bool {
        self.files
            .iter()
            .any(|file| file.state == ClientDiffFileState::Loading)
    }
}

pub(crate) fn use_client_diff_workspace(
    source: ClientDiffSource,
    identity: ViewerViewIdentity,
    files: &[ViewerFileSummary],
    reload: u64,
) -> Signal<ClientDiffWorkspace> {
    let files = files.to_owned();
    let initial_files = files.clone();
    let mut workspace = use_signal(move || ClientDiffWorkspace::loading(identity, initial_files));
    let mut generation = use_signal(ClientDiffRequestTicket::default);

    use_effect(use_reactive(
        (&source, &identity, &files, &reload),
        move |(source, identity, files, _reload)| {
            let request_generation = {
                let mut current = generation.write();
                let next = current.next();
                *current = next;
                next
            };
            workspace.set(ClientDiffWorkspace::loading(identity, files.clone()));

            spawn(async move {
                load_workspace(
                    workspace,
                    generation,
                    request_generation,
                    source,
                    identity,
                    files,
                )
                .await;
            });
        },
    ));

    workspace
}

#[derive(Clone, Copy)]
struct ClientDiffLoad {
    workspace: Signal<ClientDiffWorkspace>,
    generation: Signal<ClientDiffRequestTicket>,
    request_generation: ClientDiffRequestTicket,
    source: ClientDiffSource,
    identity: ViewerViewIdentity,
}

impl ClientDiffLoad {
    fn is_current(self) -> bool {
        *self.generation.peek() == self.request_generation
    }

    fn update_file(self, file_index: usize, update: impl FnOnce(&mut ClientDiffFile)) {
        if !self.is_current() {
            return;
        }
        let mut workspace = self.workspace;
        let mut current = workspace.write();
        if current.identity != self.identity {
            return;
        }
        if let Some(file) = current.files.get_mut(file_index) {
            update(file);
        }
    }

    async fn load_file(self, file_index: usize, file: &ViewerFileSummary) {
        let path = file.path.to_string_lossy();
        let syntax = SyntaxLanguage::from_path(path.as_ref());
        let mut parser = DiffParser::new().with_syntax(syntax).stream();
        let mut split = (self.identity.render_options.layout == ViewerDiffLayout::Split)
            .then(SplitDiffStream::new);
        let mut cursor = ViewerDiffCursor::default();

        loop {
            if !self.is_current() {
                return;
            }
            let request = LoadViewerDiffLines {
                identity: self.identity,
                file: file.id.clone(),
                cursor,
            };
            let page = match self.source.load_diff_lines(request.clone()).await {
                Ok(page) => page,
                Err(error) => {
                    self.update_file(file_index, |file| {
                        file.state = ClientDiffFileState::Error(ClientDiffFileError::Source(error));
                    });
                    return;
                }
            };
            let Ok(next) = validate_page(&request, &page) else {
                self.update_file(file_index, |file| {
                    file.state = ClientDiffFileState::Error(ClientDiffFileError::InvalidPage);
                });
                return;
            };

            for lines in page.lines.chunks(CLIENT_LINE_BATCH_SIZE) {
                let parsed = parser.push(lines);
                let line_number_digits = parsed.line_number_digits();
                let rows = parsed.into_rows();
                self.update_file(file_index, |file| {
                    file.line_number_digits = line_number_digits;
                    match &mut split {
                        Some(split) => file.rows.append_split(split.push(rows)),
                        None => file.rows.append_unified(rows),
                    }
                });
                yield_to_browser().await;
                if !self.is_current() {
                    return;
                }
            }

            let Some(next) = next else {
                let parsed = parser.finish();
                let line_number_digits = parsed.line_number_digits();
                let rows = parsed.into_rows();
                match split {
                    Some(mut split) => {
                        let mut trailing_rows = split.push(rows);
                        trailing_rows.extend(split.finish());
                        self.update_file(file_index, |file| {
                            file.line_number_digits = line_number_digits;
                            file.rows.append_split(trailing_rows);
                            file.state = ClientDiffFileState::Complete;
                        });
                    }
                    None => self.update_file(file_index, |file| {
                        file.line_number_digits = line_number_digits;
                        file.rows.append_unified(rows);
                        file.state = ClientDiffFileState::Complete;
                    }),
                }
                return;
            };
            cursor = next;
        }
    }
}

async fn load_workspace(
    workspace: Signal<ClientDiffWorkspace>,
    generation: Signal<ClientDiffRequestTicket>,
    request_generation: ClientDiffRequestTicket,
    source: ClientDiffSource,
    identity: ViewerViewIdentity,
    files: Vec<ViewerFileSummary>,
) {
    let load = ClientDiffLoad {
        workspace,
        generation,
        request_generation,
        source,
        identity,
    };

    for (file_index, file) in files.into_iter().enumerate() {
        if !load.is_current() {
            return;
        }
        load.load_file(file_index, &file).await;
    }
}

fn validate_page(
    request: &LoadViewerDiffLines,
    page: &ViewerDiffLines,
) -> Result<Option<ViewerDiffCursor>, ()> {
    if page.identity != request.identity
        || page.file != request.file
        || page.cursor != request.cursor
    {
        return Err(());
    }

    let byte_count = page
        .lines
        .iter()
        .try_fold(0_usize, |total, line| total.checked_add(line.len()))
        .ok_or(())?;
    if byte_count > VIEWER_DIFF_LINES_PAGE_MAX_BYTES && page.lines.len() != 1 {
        return Err(());
    }

    let line_count = u32::try_from(page.lines.len()).map_err(|_| ())?;
    let expected_next = request
        .cursor
        .into_inner()
        .checked_add(line_count)
        .ok_or(())?;
    if let Some(next) = page.next
        && (page.lines.is_empty() || next.into_inner() != expected_next)
    {
        return Err(());
    }
    Ok(page.next)
}

async fn yield_to_browser() {
    dioxus_sdk_time::sleep(Duration::ZERO).await;
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{ViewerRangeGeneration, ViewerSelectionGeneration};
    use gtl_wire::viewer::{
        VIEWER_DIFF_LINES_PAGE_MAX_BYTES, ViewerDiffDensity, ViewerDiffFileId, ViewerRenderOptions,
    };

    use super::*;
    use crate::test_support::{
        TestResult, absolute_file_path, repository_relative_path, viewer_tab_id,
    };

    fn identity() -> TestResult<ViewerViewIdentity> {
        Ok(ViewerViewIdentity {
            tab_id: viewer_tab_id(7)?,
            range_generation: ViewerRangeGeneration::new(11),
            selection_generation: ViewerSelectionGeneration::new(13),
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        })
    }

    fn request(cursor: u32) -> TestResult<LoadViewerDiffLines> {
        Ok(LoadViewerDiffLines {
            identity: identity()?,
            file: ViewerDiffFileId::for_index(2),
            cursor: ViewerDiffCursor::new(cursor),
        })
    }

    fn page(
        request: &LoadViewerDiffLines,
        lines: Vec<String>,
        next: Option<u32>,
    ) -> ViewerDiffLines {
        ViewerDiffLines {
            identity: request.identity,
            file: request.file.clone(),
            cursor: request.cursor,
            lines,
            next: next.map(ViewerDiffCursor::new),
        }
    }

    #[test]
    fn accepts_a_bounded_progressing_page() -> TestResult {
        let request = request(4)?;
        let page = page(&request, vec!["a".into(), "b".into()], Some(6));

        assert_eq!(
            validate_page(&request, &page),
            Ok(Some(ViewerDiffCursor::new(6)))
        );
        Ok(())
    }

    #[test]
    fn accepts_one_oversized_line_so_the_stream_can_progress() -> TestResult {
        let request = request(0)?;
        let page = page(
            &request,
            vec!["x".repeat(VIEWER_DIFF_LINES_PAGE_MAX_BYTES + 1)],
            None,
        );

        assert_eq!(validate_page(&request, &page), Ok(None));
        Ok(())
    }

    #[test]
    fn rejects_identity_file_cursor_and_progress_mismatches() -> TestResult {
        let request = request(4)?;
        let mut wrong_identity = page(&request, vec!["a".into()], Some(5));
        wrong_identity.identity.tab_id = viewer_tab_id(8)?;
        let mut wrong_file = page(&request, vec!["a".into()], Some(5));
        wrong_file.file = ViewerDiffFileId::for_index(9);
        let mut wrong_cursor = page(&request, vec!["a".into()], Some(5));
        wrong_cursor.cursor = ViewerDiffCursor::new(3);
        let wrong_next = page(&request, vec!["a".into()], Some(6));
        let empty_progress = page(&request, Vec::new(), Some(5));

        for invalid in [
            wrong_identity,
            wrong_file,
            wrong_cursor,
            wrong_next,
            empty_progress,
        ] {
            assert_eq!(validate_page(&request, &invalid), Err(()));
        }
        Ok(())
    }

    #[test]
    fn rejects_multi_line_pages_over_the_transport_cap() -> TestResult {
        let request = request(0)?;
        let page = page(
            &request,
            vec!["x".repeat(VIEWER_DIFF_LINES_PAGE_MAX_BYTES), "y".to_owned()],
            None,
        );

        assert_eq!(validate_page(&request, &page), Err(()));
        Ok(())
    }

    #[test]
    fn copied_rows_keep_new_side_source_and_context() -> TestResult {
        let summary = ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: repository_relative_path("src/example.rs")?,
            absolute_path: absolute_file_path("/repo/src/example.rs")?,
            anchor_id: "file-src-example-rs".into(),
            added: gtl_models::diffs::DiffLineCount::new(1),
            removed: gtl_models::diffs::DiffLineCount::new(1),
            status: gtl_wire::viewer::ViewerFileStatus::Modified,
            can_open_in_editor: true,
            initially_expanded: true,
        };
        let parsed = DiffParser::new().parse(&[
            "@@ -3,2 +7,2 @@".into(),
            " keep".into(),
            "-old".into(),
            "+new".into(),
        ]);
        let file = ClientDiffFile {
            summary,
            rows: ClientDiffRows::Unified(vec![Arc::new(parsed.into_rows())]),
            line_number_digits: LineNumberDigitWidth::default(),
            state: ClientDiffFileState::Complete,
        };

        assert_eq!(file.copy_code(false), "keep\nnew");
        assert_eq!(
            file.copy_code(true),
            "// * src/example.rs, lines: 7..8\nkeep\nnew"
        );
        Ok(())
    }

    #[test]
    fn shell_copy_context_uses_hash_comment_syntax() -> TestResult {
        assert_eq!(comment_leader(&repository_relative_path("script.SH")?), "#");
        assert_eq!(
            comment_leader(&repository_relative_path("src/lib.rs")?),
            "//"
        );
        Ok(())
    }

    #[test]
    fn parser_output_is_rebatched_to_the_render_limit() {
        let mut source = vec!["@@ -1,128 +1,128 @@".to_owned()];
        source.extend((0..128).map(|index| format!(" let value_{index} = {index};")));
        let parsed = DiffParser::new().parse(&source);
        let split_rows = parsed.split_rows();

        let mut unified = ClientDiffRows::Unified(Vec::new());
        unified.append_unified(parsed.into_rows());
        let unified_lengths = match unified {
            ClientDiffRows::Unified(unified) => {
                unified.iter().map(|batch| batch.len()).collect::<Vec<_>>()
            }
            ClientDiffRows::Split(_) => Vec::new(),
        };
        assert_eq!(unified_lengths, vec![64, 64, 1]);

        let mut split = ClientDiffRows::Split(Vec::new());
        split.append_split(split_rows);
        let split_lengths = match split {
            ClientDiffRows::Split(split) => {
                split.iter().map(|batch| batch.len()).collect::<Vec<_>>()
            }
            ClientDiffRows::Unified(_) => Vec::new(),
        };
        assert_eq!(split_lengths, vec![64, 64, 1]);
    }
}
