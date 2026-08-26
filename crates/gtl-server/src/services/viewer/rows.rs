use gtl_application::viewer::{
    rows::{ViewerFileRowParser, ViewerRowBatch, ViewerSyntaxDiagnostic},
    shell, viewer_diff_file_source,
};
use gtl_wire::{
    proto::viewer as viewer_proto,
    v1,
    viewer::{
        VIEWER_ROW_BATCH_MAX_ENCODED_BYTES, VIEWER_ROW_BATCH_MAX_ROWS,
        VIEWER_ROW_MAX_ENCODED_BYTES, ViewerDiffFileId, ViewerDiffLayout, ViewerRows,
        ViewerSplitRow, ViewerUnifiedRow, ViewerViewIdentity,
    },
};
use prost::Message as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::{load_user_settings, parse_identity};
use crate::state::AppState;

const SOURCE_CHUNK_LINES: usize = 64;
const ROW_STREAM_BUFFER: usize = 8;

pub(super) type RowStream = ReceiverStream<Result<v1::StreamViewerRowsResponse, Status>>;

pub(super) fn start(
    state: AppState,
    request: v1::StreamViewerRowsRequest,
) -> Result<RowStream, Status> {
    let proto_identity = request
        .identity
        .ok_or_else(|| Status::invalid_argument("identity is required"))?;
    let identity = parse_identity(&proto_identity)?;
    let options = load_user_settings(&state)?.viewer_render_options();
    if gtl_application::viewer::project_render_options(options) != identity.render_options {
        return Err(Status::aborted("viewer identity changed"));
    }
    let snapshot = shell::content_snapshot_for_identity(&state.viewer, identity, options)
        .map_err(|error| super::viewer_state_error(error, "load viewer rows"))?
        .ok_or_else(|| Status::aborted("viewer identity changed"))?;
    let all_files = (0..snapshot.view().files.len())
        .map(ViewerDiffFileId::for_index)
        .collect::<Vec<_>>();
    let files = match request.file_id {
        Some(requested) => vec![
            all_files
                .into_iter()
                .find(|file| file.as_str() == requested)
                .ok_or_else(|| Status::not_found("viewer diff file is not available"))?,
        ],
        None => all_files,
    };
    let row_stream = state
        .viewer_row_streams
        .start_stream()
        .map_err(|_| Status::resource_exhausted("viewer row stream counter is exhausted"))?;
    let (sender, receiver) = mpsc::channel(ROW_STREAM_BUFFER);
    tokio::task::spawn_blocking(move || {
        produce_rows(
            state,
            identity,
            proto_identity,
            snapshot.view(),
            files,
            row_stream,
            sender,
        );
    });
    Ok(ReceiverStream::new(receiver))
}

fn produce_rows(
    state: AppState,
    identity: ViewerViewIdentity,
    proto_identity: v1::ViewerViewIdentity,
    view: &gtl_application::diffs::View,
    files: Vec<ViewerDiffFileId>,
    row_stream: u64,
    sender: mpsc::Sender<Result<v1::StreamViewerRowsResponse, Status>>,
) {
    let mut writer = StreamWriter {
        state,
        identity,
        proto_identity,
        sequence: 0,
        row_stream,
        sender,
    };
    for file_id in files {
        if !writer.send(v1::stream_viewer_rows_response::Event::FileStarted(
            v1::ViewerFileStarted {
                file_id: file_id.as_str().to_owned(),
            },
        )) {
            return;
        }
        let Some(source) = viewer_diff_file_source(view, &file_id, identity.render_options.density)
        else {
            if !writer.file_failed(
                &file_id,
                v1::ViewerFileFailureCode::SourceUnavailable,
                "The diff source is no longer available.",
                false,
            ) {
                return;
            }
            continue;
        };
        let mut parser = ViewerFileRowParser::new(identity.render_options.layout, source.path);
        let mut failed = false;

        for lines in source.lines.chunks(SOURCE_CHUNK_LINES) {
            let result = send_projected_rows(&mut writer, &file_id, parser.push(lines));
            match result {
                BatchResult::Sent => {}
                BatchResult::Cancelled => return,
                BatchResult::RowTooLarge => {
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            if !writer.file_failed(
                &file_id,
                v1::ViewerFileFailureCode::RowTooLarge,
                "A diff row is too large to display.",
                false,
            ) {
                return;
            }
            continue;
        }

        let parsed = parser.finish();
        let line_number_digits = parsed.line_number_digits;
        let result = send_projected_rows(&mut writer, &file_id, parsed);
        if result == BatchResult::Cancelled {
            return;
        }
        if result == BatchResult::RowTooLarge {
            if !writer.file_failed(
                &file_id,
                v1::ViewerFileFailureCode::RowTooLarge,
                "A diff row is too large to display.",
                false,
            ) {
                return;
            }
            continue;
        }
        if !writer.send(v1::stream_viewer_rows_response::Event::FileFinished(
            v1::ViewerFileFinished {
                file_id: file_id.as_str().to_owned(),
                line_number_digits,
            },
        )) {
            return;
        }
    }
}

struct StreamWriter {
    state: AppState,
    identity: ViewerViewIdentity,
    proto_identity: v1::ViewerViewIdentity,
    sequence: u64,
    row_stream: u64,
    sender: mpsc::Sender<Result<v1::StreamViewerRowsResponse, Status>>,
}

impl StreamWriter {
    fn is_current(&self) -> bool {
        if !self.state.viewer_row_streams.is_current(self.row_stream) {
            return false;
        }
        shell::identity_is_current(
            &self.state.viewer,
            self.identity,
            to_render_options(self.identity.render_options),
        )
        .unwrap_or(false)
    }

    fn response(
        &self,
        event: v1::stream_viewer_rows_response::Event,
    ) -> v1::StreamViewerRowsResponse {
        v1::StreamViewerRowsResponse {
            identity: Some(self.proto_identity),
            sequence: self.sequence,
            event: Some(event),
        }
    }

    fn send(&mut self, event: v1::stream_viewer_rows_response::Event) -> bool {
        if !self.is_current() {
            return false;
        }
        let Some(next_sequence) = self.sequence.checked_add(1) else {
            let _ = self.sender.blocking_send(Err(Status::resource_exhausted(
                "viewer row sequence is exhausted",
            )));
            return false;
        };
        let response = self.response(event);
        if self.sender.blocking_send(Ok(response)).is_err() {
            return false;
        }
        self.sequence = next_sequence;
        true
    }

    fn file_failed(
        &mut self,
        file_id: &ViewerDiffFileId,
        code: v1::ViewerFileFailureCode,
        message: &str,
        retryable: bool,
    ) -> bool {
        self.send(v1::stream_viewer_rows_response::Event::FileFailed(
            v1::ViewerFileFailed {
                file_id: file_id.as_str().to_owned(),
                code: code as i32,
                message: message.to_owned(),
                retryable,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatchResult {
    Sent,
    Cancelled,
    RowTooLarge,
}

fn send_projected_rows(
    writer: &mut StreamWriter,
    file_id: &ViewerDiffFileId,
    batch: ViewerRowBatch,
) -> BatchResult {
    log_syntax_diagnostics(file_id, &batch.diagnostics);
    match batch.rows {
        ViewerRows::Unified(rows) => send_unified_rows(writer, file_id, rows),
        ViewerRows::Split(rows) => send_split_rows(writer, file_id, rows),
    }
}

fn send_unified_rows(
    writer: &mut StreamWriter,
    file_id: &ViewerDiffFileId,
    rows: Vec<ViewerUnifiedRow>,
) -> BatchResult {
    let Ok(rows) = rows
        .into_iter()
        .map(viewer_proto::encode_viewer_unified_row)
        .collect::<Result<Vec<_>, _>>()
    else {
        return BatchResult::RowTooLarge;
    };
    send_bounded(writer, rows, |rows| {
        v1::stream_viewer_rows_response::Event::UnifiedRows(v1::ViewerUnifiedRows {
            file_id: file_id.as_str().to_owned(),
            rows,
        })
    })
}

fn send_split_rows(
    writer: &mut StreamWriter,
    file_id: &ViewerDiffFileId,
    rows: Vec<ViewerSplitRow>,
) -> BatchResult {
    let Ok(rows) = rows
        .into_iter()
        .map(viewer_proto::encode_viewer_split_row)
        .collect::<Result<Vec<_>, _>>()
    else {
        return BatchResult::RowTooLarge;
    };
    send_bounded(writer, rows, |rows| {
        v1::stream_viewer_rows_response::Event::SplitRows(v1::ViewerSplitRows {
            file_id: file_id.as_str().to_owned(),
            rows,
        })
    })
}

fn send_bounded<Row>(
    writer: &mut StreamWriter,
    rows: Vec<Row>,
    event: impl Fn(Vec<Row>) -> v1::stream_viewer_rows_response::Event,
) -> BatchResult
where
    Row: prost::Message + Clone,
{
    let mut batch = Vec::new();
    for row in rows {
        if row.encoded_len() > VIEWER_ROW_MAX_ENCODED_BYTES {
            return BatchResult::RowTooLarge;
        }
        let mut candidate = batch.clone();
        candidate.push(row.clone());
        let candidate_response = writer.response(event(candidate));
        let crosses_bound = !batch.is_empty()
            && (batch.len() >= VIEWER_ROW_BATCH_MAX_ROWS
                || candidate_response.encoded_len() > VIEWER_ROW_BATCH_MAX_ENCODED_BYTES);
        if crosses_bound && !writer.send(event(std::mem::take(&mut batch))) {
            return BatchResult::Cancelled;
        }
        batch.push(row);
        if batch.len() == 1 {
            let single = writer.response(event(batch.clone()));
            if single.encoded_len() > VIEWER_ROW_BATCH_MAX_ENCODED_BYTES
                && !writer.send(event(std::mem::take(&mut batch)))
            {
                return BatchResult::Cancelled;
            }
        }
    }
    if !batch.is_empty() && !writer.send(event(batch)) {
        return BatchResult::Cancelled;
    }
    BatchResult::Sent
}

fn to_render_options(
    options: gtl_wire::viewer::ViewerRenderOptions,
) -> gtl_models::viewer::RenderOptions {
    gtl_models::viewer::RenderOptions::new(
        match options.layout {
            ViewerDiffLayout::Unified => gtl_models::viewer::DiffLayout::Unified,
            ViewerDiffLayout::Split => gtl_models::viewer::DiffLayout::Split,
        },
        match options.density {
            gtl_wire::viewer::ViewerDiffDensity::Compact => {
                gtl_models::viewer::DiffDensity::Compact
            }
            gtl_wire::viewer::ViewerDiffDensity::Full => gtl_models::viewer::DiffDensity::Full,
        },
    )
}

fn log_syntax_diagnostics(file: &ViewerDiffFileId, diagnostics: &[ViewerSyntaxDiagnostic]) {
    for diagnostic in diagnostics {
        tracing::warn!(
            file_id = file.as_str(),
            side = ?diagnostic.side,
            error = diagnostic.message,
            "viewer syntax highlighting skipped part of a diff"
        );
    }
}
