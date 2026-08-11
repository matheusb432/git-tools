use std::{path::Path, sync::Arc, time::Duration};

use dioxus::prelude::*;
use gtl_contracts::viewer::{
    LoadViewerDiffLines, VIEWER_DIFF_LINES_PAGE_MAX_BYTES, ViewerDiffCursor, ViewerDiffLayout,
    ViewerDiffLines, ViewerFileSummary, ViewerViewIdentity,
};
use gtl_parser::{
    DiffParser, DiffRow, SplitDiffRow, SplitDiffStream, SyntaxCatalog, bundled_syntax_catalog,
};

use self::source::ClientDiffSourceError;

mod source;

pub(crate) use source::ClientDiffSource;

const CLIENT_LINE_BATCH_SIZE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientDiffRows {
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
        if rows.is_empty() {
            return;
        }
        if let Self::Unified(batches) = self {
            batches.push(Arc::new(rows));
        }
    }

    fn append_split(&mut self, rows: Vec<SplitDiffRow>) {
        if rows.is_empty() {
            return;
        }
        if let Self::Split(batches) = self {
            batches.push(Arc::new(rows));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClientDiffFileError {
    Source(ClientDiffSourceError),
    InvalidPage,
}

impl ClientDiffFileError {
    pub(crate) const fn message(self) -> &'static str {
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
    pub(crate) line_number_digits: u32,
    pub(crate) state: ClientDiffFileState,
}

impl ClientDiffFile {
    fn loading(summary: ViewerFileSummary, layout: ViewerDiffLayout) -> Self {
        Self {
            summary,
            rows: ClientDiffRows::new(layout),
            line_number_digits: 1,
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
            self.summary.path,
            line_range
                .map(|range| format!(", lines: {range}"))
                .unwrap_or_default(),
        )
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct CopiedRows {
    lines: Vec<String>,
    first_line: Option<u32>,
    last_line: Option<u32>,
}

impl CopiedRows {
    fn push(&mut self, line: String, line_number: Option<u32>) {
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

fn comment_leader(path: &str) -> &'static str {
    let extension = Path::new(path)
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
    files: Vec<ViewerFileSummary>,
    reload: u64,
) -> Signal<ClientDiffWorkspace> {
    let initial_files = files.clone();
    let mut workspace = use_signal(move || ClientDiffWorkspace::loading(identity, initial_files));
    let mut generation = use_signal(|| 0_u64);

    use_effect(use_reactive(
        (&source, &identity, &files, &reload),
        move |(source, identity, files, _reload)| {
            let request_generation = {
                let mut current = generation.write();
                *current += 1;
                *current
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

async fn load_workspace(
    workspace: Signal<ClientDiffWorkspace>,
    generation: Signal<u64>,
    request_generation: u64,
    source: ClientDiffSource,
    identity: ViewerViewIdentity,
    files: Vec<ViewerFileSummary>,
) {
    let syntax_catalog = bundled_syntax_catalog().ok();

    for (file_index, file) in files.into_iter().enumerate() {
        if !is_current(generation, request_generation) {
            return;
        }
        load_file(
            workspace,
            generation,
            request_generation,
            source,
            identity,
            file_index,
            &file,
            syntax_catalog.as_ref(),
        )
        .await;
    }
}

async fn load_file(
    mut workspace: Signal<ClientDiffWorkspace>,
    generation: Signal<u64>,
    request_generation: u64,
    source: ClientDiffSource,
    identity: ViewerViewIdentity,
    file_index: usize,
    file: &ViewerFileSummary,
    syntax_catalog: Option<&SyntaxCatalog>,
) {
    let syntax = syntax_catalog.and_then(|catalog| catalog.syntax_for_path(&file.path));
    let mut parser = DiffParser::new().with_syntax(syntax).stream();
    let mut split =
        (identity.render_options.layout == ViewerDiffLayout::Split).then(SplitDiffStream::new);
    let mut cursor = ViewerDiffCursor::START;

    loop {
        if !is_current(generation, request_generation) {
            return;
        }
        let request = LoadViewerDiffLines {
            identity,
            file: file.id.clone(),
            cursor,
        };
        let page = match source.load_diff_lines(request.clone()).await {
            Ok(page) => page,
            Err(error) => {
                update_file(
                    &mut workspace,
                    generation,
                    request_generation,
                    identity,
                    file_index,
                    |file| {
                        file.state = ClientDiffFileState::Error(ClientDiffFileError::Source(error))
                    },
                );
                return;
            }
        };
        let next = match validate_page(&request, &page) {
            Ok(next) => next,
            Err(()) => {
                update_file(
                    &mut workspace,
                    generation,
                    request_generation,
                    identity,
                    file_index,
                    |file| {
                        file.state = ClientDiffFileState::Error(ClientDiffFileError::InvalidPage)
                    },
                );
                return;
            }
        };

        for lines in page.lines.chunks(CLIENT_LINE_BATCH_SIZE) {
            let parsed = parser.push(lines);
            let line_number_digits = parsed.line_number_digits();
            let rows = parsed.into_rows();
            update_file(
                &mut workspace,
                generation,
                request_generation,
                identity,
                file_index,
                |file| {
                    file.line_number_digits = line_number_digits;
                    match &mut split {
                        Some(split) => file.rows.append_split(split.push(rows)),
                        None => file.rows.append_unified(rows),
                    }
                },
            );
            yield_to_browser().await;
            if !is_current(generation, request_generation) {
                return;
            }
        }

        let Some(next) = next else {
            let trailing_rows = split.map(SplitDiffStream::finish);
            update_file(
                &mut workspace,
                generation,
                request_generation,
                identity,
                file_index,
                |file| {
                    if let Some(rows) = trailing_rows {
                        file.rows.append_split(rows);
                    }
                    file.state = ClientDiffFileState::Complete;
                },
            );
            return;
        };
        cursor = next;
    }
}

fn update_file(
    workspace: &mut Signal<ClientDiffWorkspace>,
    generation: Signal<u64>,
    request_generation: u64,
    identity: ViewerViewIdentity,
    file_index: usize,
    update: impl FnOnce(&mut ClientDiffFile),
) {
    if !is_current(generation, request_generation) {
        return;
    }
    let mut current = workspace.write();
    if current.identity != identity {
        return;
    }
    if let Some(file) = current.files.get_mut(file_index) {
        update(file);
    }
}

fn is_current(generation: Signal<u64>, request_generation: u64) -> bool {
    generation() == request_generation
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
        .position()
        .checked_add(line_count)
        .ok_or(())?;
    if let Some(next) = page.next {
        if page.lines.is_empty() || next.position() != expected_next {
            return Err(());
        }
    }
    Ok(page.next)
}

async fn yield_to_browser() {
    dioxus_sdk_time::sleep(Duration::ZERO).await;
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{
        VIEWER_DIFF_LINES_PAGE_MAX_BYTES, ViewerDiffDensity, ViewerDiffFileId, ViewerRenderOptions,
    };

    use super::*;

    fn identity() -> ViewerViewIdentity {
        ViewerViewIdentity {
            tab_id: 7,
            range_generation: 11,
            selection_generation: 13,
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
        }
    }

    fn request(cursor: u32) -> LoadViewerDiffLines {
        LoadViewerDiffLines {
            identity: identity(),
            file: ViewerDiffFileId::for_index(2),
            cursor: ViewerDiffCursor::new(cursor),
        }
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
    fn accepts_a_bounded_progressing_page() {
        let request = request(4);
        let page = page(&request, vec!["a".into(), "b".into()], Some(6));

        assert_eq!(
            validate_page(&request, &page),
            Ok(Some(ViewerDiffCursor::new(6)))
        );
    }

    #[test]
    fn accepts_one_oversized_line_so_the_stream_can_progress() {
        let request = request(0);
        let page = page(
            &request,
            vec!["x".repeat(VIEWER_DIFF_LINES_PAGE_MAX_BYTES + 1)],
            None,
        );

        assert_eq!(validate_page(&request, &page), Ok(None));
    }

    #[test]
    fn rejects_identity_file_cursor_and_progress_mismatches() {
        let request = request(4);
        let mut wrong_identity = page(&request, vec!["a".into()], Some(5));
        wrong_identity.identity.tab_id += 1;
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
    }

    #[test]
    fn rejects_multi_line_pages_over_the_transport_cap() {
        let request = request(0);
        let page = page(
            &request,
            vec!["x".repeat(VIEWER_DIFF_LINES_PAGE_MAX_BYTES), "y".to_owned()],
            None,
        );

        assert_eq!(validate_page(&request, &page), Err(()));
    }

    #[test]
    fn copied_rows_keep_new_side_source_and_context() {
        let summary = ViewerFileSummary {
            id: ViewerDiffFileId::for_index(0),
            path: "src/example.rs".into(),
            absolute_path: "/repo/src/example.rs".into(),
            anchor_id: "file-src-example-rs".into(),
            added: 1,
            removed: 1,
            status: gtl_contracts::viewer::ViewerFileStatus::Modified,
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
            line_number_digits: 1,
            state: ClientDiffFileState::Complete,
        };

        assert_eq!(file.copy_code(false), "keep\nnew");
        assert_eq!(
            file.copy_code(true),
            "// * src/example.rs, lines: 7..8\nkeep\nnew"
        );
    }

    #[test]
    fn shell_copy_context_uses_hash_comment_syntax() {
        assert_eq!(comment_leader("script.SH"), "#");
        assert_eq!(comment_leader("src/lib.rs"), "//");
    }
}
