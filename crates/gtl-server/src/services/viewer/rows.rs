use gtl_application::viewer::{
    rows::{ViewerFileRowParser, ViewerRowBatch, ViewerSyntaxDiagnostic},
    shell, viewer_diff_file_source,
};
use gtl_wire::{
    proto, v1,
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
        if !produce_file_rows(&mut writer, view, &file_id) {
            return;
        }
    }
}

fn produce_file_rows(
    writer: &mut StreamWriter,
    view: &gtl_application::diffs::View,
    file_id: &ViewerDiffFileId,
) -> bool {
    if !writer.send(v1::stream_viewer_rows_response::Event::FileStarted(
        v1::ViewerFileStarted {
            file_id: file_id.as_str().to_owned(),
        },
    )) {
        return false;
    }
    let Some(source) =
        viewer_diff_file_source(view, file_id, writer.identity.render_options.density)
    else {
        return writer.file_failed(
            file_id,
            v1::ViewerFileFailureCode::SourceUnavailable,
            "The diff source is no longer available.",
            false,
        );
    };
    let mut parser = ViewerFileRowParser::new(writer.identity.render_options.layout, source.path);

    for lines in source.lines.chunks(SOURCE_CHUNK_LINES) {
        match send_projected_rows(writer, file_id, parser.push(lines)) {
            BatchResult::Sent => {}
            BatchResult::Cancelled => return false,
            BatchResult::RowTooLarge => return report_oversized_row(writer, file_id),
        }
    }

    let parsed = parser.finish();
    let line_number_digits = parsed.line_number_digits;
    match send_projected_rows(writer, file_id, parsed) {
        BatchResult::Sent => writer.send(v1::stream_viewer_rows_response::Event::FileFinished(
            v1::ViewerFileFinished {
                file_id: file_id.as_str().to_owned(),
                line_number_digits,
            },
        )),
        BatchResult::Cancelled => false,
        BatchResult::RowTooLarge => report_oversized_row(writer, file_id),
    }
}

fn report_oversized_row(writer: &mut StreamWriter, file_id: &ViewerDiffFileId) -> bool {
    writer.file_failed(
        file_id,
        v1::ViewerFileFailureCode::RowTooLarge,
        "A diff row is too large to display.",
        false,
    )
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
        .map(proto::viewer::encode_viewer_unified_row)
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
        .map(proto::viewer::encode_viewer_split_row)
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
    Row: prost::Message,
{
    let empty_event = event(Vec::new());
    let Some(empty_event_encoded_len) = row_event_encoded_len(&empty_event) else {
        return BatchResult::RowTooLarge;
    };
    let mut empty_response_encoded_len = writer.response(empty_event).encoded_len();
    let mut batch = Vec::new();
    let mut rows_encoded_len = 0_usize;
    for row in rows {
        let result = append_bounded_row(
            writer,
            &mut batch,
            row,
            &mut rows_encoded_len,
            &mut empty_response_encoded_len,
            empty_event_encoded_len,
            &event,
        );
        if result != BatchResult::Sent {
            return result;
        }
    }
    if !batch.is_empty() && !writer.send(event(batch)) {
        return BatchResult::Cancelled;
    }
    BatchResult::Sent
}

fn append_bounded_row<Row>(
    writer: &mut StreamWriter,
    batch: &mut Vec<Row>,
    row: Row,
    rows_encoded_len: &mut usize,
    empty_response_encoded_len: &mut usize,
    empty_event_encoded_len: usize,
    event: &impl Fn(Vec<Row>) -> v1::stream_viewer_rows_response::Event,
) -> BatchResult
where
    Row: prost::Message,
{
    if row.encoded_len() > VIEWER_ROW_MAX_ENCODED_BYTES {
        return BatchResult::RowTooLarge;
    }
    let framed_row_len = prost::encoding::message::encoded_len(2, &row);
    let Some(candidate_rows_encoded_len) = rows_encoded_len.checked_add(framed_row_len) else {
        return BatchResult::RowTooLarge;
    };
    let Some(candidate_response_len) = row_batch_response_encoded_len(
        *empty_response_encoded_len,
        empty_event_encoded_len,
        candidate_rows_encoded_len,
    ) else {
        return BatchResult::RowTooLarge;
    };
    let crosses_bound = !batch.is_empty()
        && (batch.len() >= VIEWER_ROW_BATCH_MAX_ROWS
            || candidate_response_len > VIEWER_ROW_BATCH_MAX_ENCODED_BYTES);
    if crosses_bound && !writer.send(event(std::mem::take(batch))) {
        return BatchResult::Cancelled;
    }
    if crosses_bound {
        *rows_encoded_len = 0;
        *empty_response_encoded_len = writer.response(event(Vec::new())).encoded_len();
    }

    let Some(next_rows_encoded_len) = rows_encoded_len.checked_add(framed_row_len) else {
        return BatchResult::RowTooLarge;
    };
    *rows_encoded_len = next_rows_encoded_len;
    batch.push(row);
    if batch.len() != 1 {
        return BatchResult::Sent;
    }
    let Some(single_response_len) = row_batch_response_encoded_len(
        *empty_response_encoded_len,
        empty_event_encoded_len,
        *rows_encoded_len,
    ) else {
        return BatchResult::RowTooLarge;
    };
    if single_response_len <= VIEWER_ROW_BATCH_MAX_ENCODED_BYTES {
        return BatchResult::Sent;
    }
    if !writer.send(event(std::mem::take(batch))) {
        return BatchResult::Cancelled;
    }
    *rows_encoded_len = 0;
    *empty_response_encoded_len = writer.response(event(Vec::new())).encoded_len();
    BatchResult::Sent
}

fn row_event_encoded_len(event: &v1::stream_viewer_rows_response::Event) -> Option<usize> {
    match event {
        v1::stream_viewer_rows_response::Event::UnifiedRows(rows) => Some(rows.encoded_len()),
        v1::stream_viewer_rows_response::Event::SplitRows(rows) => Some(rows.encoded_len()),
        v1::stream_viewer_rows_response::Event::FileStarted(_)
        | v1::stream_viewer_rows_response::Event::FileFinished(_)
        | v1::stream_viewer_rows_response::Event::FileFailed(_) => None,
    }
}

fn row_batch_response_encoded_len(
    empty_response_encoded_len: usize,
    empty_event_encoded_len: usize,
    rows_encoded_len: usize,
) -> Option<usize> {
    let event_encoded_len = empty_event_encoded_len.checked_add(rows_encoded_len)?;
    let empty_event_prefix_len =
        prost::encoding::encoded_len_varint(u64::try_from(empty_event_encoded_len).ok()?);
    let event_prefix_len =
        prost::encoding::encoded_len_varint(u64::try_from(event_encoded_len).ok()?);
    empty_response_encoded_len
        .checked_add(rows_encoded_len)?
        .checked_add(event_prefix_len)?
        .checked_sub(empty_event_prefix_len)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_row_batch_size_matches_protobuf_encoding() {
        let rows = vec![
            v1::ViewerUnifiedRow {
                row: Some(v1::viewer_unified_row::Row::Meta("short".to_owned())),
            },
            v1::ViewerUnifiedRow {
                row: Some(v1::viewer_unified_row::Row::Meta("x".repeat(200))),
            },
        ];
        let empty_event = v1::ViewerUnifiedRows {
            file_id: "file-0".to_owned(),
            rows: Vec::new(),
        };
        let response = |rows| v1::StreamViewerRowsResponse {
            identity: Some(v1::ViewerViewIdentity::default()),
            sequence: 7,
            event: Some(v1::stream_viewer_rows_response::Event::UnifiedRows(
                v1::ViewerUnifiedRows {
                    file_id: empty_event.file_id.clone(),
                    rows,
                },
            )),
        };
        let empty_response_encoded_len = response(Vec::new()).encoded_len();
        let rows_encoded_len = rows
            .iter()
            .map(|row| prost::encoding::message::encoded_len(2, row))
            .sum();

        assert_eq!(
            row_batch_response_encoded_len(
                empty_response_encoded_len,
                empty_event.encoded_len(),
                rows_encoded_len,
            ),
            Some(response(rows).encoded_len())
        );
    }
}
