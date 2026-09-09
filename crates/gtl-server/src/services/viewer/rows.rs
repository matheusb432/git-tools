use gtl_application::viewer::{
    rows::{
        ViewerRowWindowError, ViewerRowWindowParser, ViewerWorkCancellation, viewer_file_row_count,
    },
    shell, viewer_diff_file_source,
};
use gtl_wire::{
    proto, v1,
    viewer::{
        VIEWER_ROW_BATCH_MAX_ENCODED_BYTES, VIEWER_ROW_BATCH_MAX_ROWS,
        VIEWER_ROW_MAX_ENCODED_BYTES, ViewerDiffFileId, ViewerDiffLayout, ViewerRowRange,
        ViewerRows, ViewerSplitRow, ViewerUnifiedRow, ViewerViewIdentity,
    },
};
use prost::Message as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::{load_user_settings, parse_identity};
use crate::state::AppState;

const ROW_STREAM_BUFFER: usize = 8;

pub(crate) struct RowStream {
    receiver: ReceiverStream<Result<v1::StreamViewerRowsResponse, Status>>,
    _cancel_on_drop: super::cancellation::CancelOnDrop,
}

impl tokio_stream::Stream for RowStream {
    type Item = Result<v1::StreamViewerRowsResponse, Status>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.receiver).poll_next(context)
    }
}

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
    if identity.render_options.density == gtl_wire::viewer::ViewerDiffDensity::Full
        && matches!(
            snapshot.view().full_context,
            gtl_application::diffs::FullContextDiffState::Deferred(_)
        )
    {
        return Err(Status::failed_precondition(
            "the diff source is still being prepared",
        ));
    }
    let range = request
        .row_range
        .map(|range| ViewerRowRange::try_new(range.start, range.count))
        .transpose()
        .map_err(|_| Status::invalid_argument("viewer row range is invalid"))?;
    if range.is_some() && request.file_id.is_none() {
        return Err(Status::invalid_argument("a row range requires one file"));
    }
    let files = match request.file_id {
        Some(requested) => {
            let file: ViewerDiffFileId = requested
                .try_into()
                .map_err(|_| Status::invalid_argument("file is required"))?;
            if viewer_diff_file_source(snapshot.view(), &file, identity.render_options.density)
                .is_none()
            {
                return Err(Status::not_found("viewer diff file is not available"));
            }
            vec![file]
        }
        None => (0..snapshot.view().files.len())
            .map(ViewerDiffFileId::for_index)
            .collect(),
    };
    let row_stream = state
        .viewer_row_streams
        .start_stream()
        .map_err(|_| Status::internal("viewer work state is unavailable"))?;
    let (sender, receiver) = mpsc::channel(ROW_STREAM_BUFFER);
    let cancel_on_drop = super::cancellation::CancelOnDrop(row_stream.clone());
    let writer = StreamWriter {
        state,
        identity,
        proto_identity,
        sequence: 0,
        next_row: 0,
        row_stream,
        sender,
    };
    tokio::task::spawn_blocking(move || produce_rows(writer, snapshot.view(), files, range));
    Ok(RowStream {
        receiver: ReceiverStream::new(receiver),
        _cancel_on_drop: cancel_on_drop,
    })
}

fn produce_rows(
    mut writer: StreamWriter,
    view: &gtl_application::diffs::View,
    files: Vec<ViewerDiffFileId>,
    range: Option<ViewerRowRange>,
) {
    for file_id in files {
        if !produce_file_rows(&mut writer, view, &file_id, range) {
            return;
        }
    }
}

fn produce_file_rows(
    writer: &mut StreamWriter,
    view: &gtl_application::diffs::View,
    file_id: &ViewerDiffFileId,
    range: Option<ViewerRowRange>,
) -> bool {
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
    let row_count = viewer_file_row_count(source.lines, writer.identity.render_options.layout);
    let Ok(total) = u32::try_from(row_count) else {
        return report_oversized_row(writer, file_id);
    };
    let (start_row, end_row) = range.map_or((0, total), |range| (range.start(), range.end()));
    if end_row > total {
        let _ = writer.sender.blocking_send(Err(Status::out_of_range(
            "viewer row range is outside the file",
        )));
        return false;
    }
    writer.next_row = start_row;
    if !writer.send(v1::stream_viewer_rows_response::Event::FileStarted(
        v1::ViewerFileStarted {
            file_id: file_id.as_str().to_owned(),
            row_count: total,
            start_row,
        },
    )) {
        return false;
    }

    let mut parser = ViewerRowWindowParser::new(
        source.path,
        source.lines,
        writer.identity.render_options.layout,
    );
    let mut line_number_digits = 1;
    for start in (start_row as usize..end_row as usize).step_by(VIEWER_ROW_BATCH_MAX_ROWS) {
        if !writer.is_current() {
            return false;
        }
        let parsed = parser.parse(
            start..(start + VIEWER_ROW_BATCH_MAX_ROWS).min(end_row as usize),
            &writer.row_stream,
        );
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(ViewerRowWindowError::Cancelled) => return false,
            Err(ViewerRowWindowError::InvalidRange) => {
                return writer.file_failed(
                    file_id,
                    v1::ViewerFileFailureCode::SourceUnavailable,
                    "The diff row range is unavailable.",
                    false,
                );
            }
        };
        line_number_digits = parsed.line_number_digits;
        for diagnostic in &parsed.diagnostics {
            tracing::debug!(file = file_id.as_str(), side = ?diagnostic.side, message = %diagnostic.message, "viewer syntax fallback");
        }
        match send_projected_rows(writer, file_id, parsed.rows) {
            BatchResult::Sent => {}
            BatchResult::Cancelled => return false,
            BatchResult::RowTooLarge => return report_oversized_row(writer, file_id),
        }
    }
    writer.send(v1::stream_viewer_rows_response::Event::FileFinished(
        v1::ViewerFileFinished {
            file_id: file_id.as_str().to_owned(),
            line_number_digits,
            end_row: writer.next_row,
        },
    ))
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
    next_row: u32,
    row_stream: ViewerWorkCancellation,
    sender: mpsc::Sender<Result<v1::StreamViewerRowsResponse, Status>>,
}

impl StreamWriter {
    fn is_current(&self) -> bool {
        if self.row_stream.is_cancelled() || self.sender.is_closed() {
            return false;
        }
        let current = shell::identity_is_current(
            &self.state.viewer,
            self.identity,
            to_render_options(self.identity.render_options),
        )
        .unwrap_or(false);
        if !current {
            self.row_stream.cancel();
        }
        current
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
        let count = match &event {
            v1::stream_viewer_rows_response::Event::UnifiedRows(rows) => rows.rows.len(),
            v1::stream_viewer_rows_response::Event::SplitRows(rows) => rows.rows.len(),
            _ => 0,
        };
        let Some(next_row) = u32::try_from(count)
            .ok()
            .and_then(|count| self.next_row.checked_add(count))
        else {
            return false;
        };
        let response = self.response(event);
        if self.sender.blocking_send(Ok(response)).is_err() {
            return false;
        }
        self.sequence = next_sequence;
        self.next_row = next_row;
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
    rows: ViewerRows,
) -> BatchResult {
    if !writer.is_current() {
        return BatchResult::Cancelled;
    }
    match rows {
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
    send_bounded(writer, rows, |rows, start_row| {
        v1::stream_viewer_rows_response::Event::UnifiedRows(v1::ViewerUnifiedRows {
            file_id: file_id.as_str().to_owned(),
            rows,
            start_row,
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
    send_bounded(writer, rows, |rows, start_row| {
        v1::stream_viewer_rows_response::Event::SplitRows(v1::ViewerSplitRows {
            file_id: file_id.as_str().to_owned(),
            rows,
            start_row,
        })
    })
}

fn send_bounded<Row>(
    writer: &mut StreamWriter,
    rows: Vec<Row>,
    event: impl Fn(Vec<Row>, u32) -> v1::stream_viewer_rows_response::Event,
) -> BatchResult
where
    Row: prost::Message,
{
    let mut batch = Vec::new();
    let mut rows_encoded_len = 0;
    for row in rows {
        if !writer.is_current() {
            return BatchResult::Cancelled;
        }
        let result = append_bounded_row(writer, &mut batch, row, &mut rows_encoded_len, &event);
        if result != BatchResult::Sent {
            return result;
        }
    }
    if !batch.is_empty() && !writer.send(event(batch, writer.next_row)) {
        return BatchResult::Cancelled;
    }
    BatchResult::Sent
}

fn append_bounded_row<Row>(
    writer: &mut StreamWriter,
    batch: &mut Vec<Row>,
    row: Row,
    rows_encoded_len: &mut usize,
    event: &impl Fn(Vec<Row>, u32) -> v1::stream_viewer_rows_response::Event,
) -> BatchResult
where
    Row: prost::Message,
{
    if row.encoded_len() > VIEWER_ROW_MAX_ENCODED_BYTES {
        return BatchResult::RowTooLarge;
    }
    let framed = prost::encoding::message::encoded_len(2, &row);
    let Some(candidate) = rows_encoded_len
        .checked_add(framed)
        .and_then(|bytes| batch_encoded_bytes(writer, bytes, event))
    else {
        return BatchResult::RowTooLarge;
    };
    if !batch.is_empty()
        && (batch.len() >= VIEWER_ROW_BATCH_MAX_ROWS
            || candidate > VIEWER_ROW_BATCH_MAX_ENCODED_BYTES)
    {
        if !writer.send(event(std::mem::take(batch), writer.next_row)) {
            return BatchResult::Cancelled;
        }
        *rows_encoded_len = 0;
    }
    *rows_encoded_len += framed;
    batch.push(row);
    if batch.len() != 1 {
        return BatchResult::Sent;
    }
    let Some(single) = batch_encoded_bytes(writer, *rows_encoded_len, event) else {
        return BatchResult::RowTooLarge;
    };
    if single <= VIEWER_ROW_BATCH_MAX_ENCODED_BYTES {
        return BatchResult::Sent;
    }
    if !writer.send(event(std::mem::take(batch), writer.next_row)) {
        return BatchResult::Cancelled;
    }
    *rows_encoded_len = 0;
    BatchResult::Sent
}

fn batch_encoded_bytes<Row>(
    writer: &StreamWriter,
    rows_bytes: usize,
    event: &impl Fn(Vec<Row>, u32) -> v1::stream_viewer_rows_response::Event,
) -> Option<usize> {
    let empty = event(Vec::new(), writer.next_row);
    let event_bytes = row_event_encoded_len(&empty)?;
    row_batch_response_encoded_len(
        writer.response(empty).encoded_len(),
        event_bytes,
        rows_bytes,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_the_grpc_stream_cancels_its_cpu_worker() {
        let cancellation = ViewerWorkCancellation::default();
        let (_sender, receiver) = mpsc::channel(1);
        let stream = RowStream {
            receiver: ReceiverStream::new(receiver),
            _cancel_on_drop: super::super::cancellation::CancelOnDrop(cancellation.clone()),
        };
        assert!(!cancellation.is_cancelled());
        drop(stream);
        assert!(cancellation.is_cancelled());
    }

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
            start_row: 0,
            file_id: "file-0".to_owned(),
            rows: Vec::new(),
        };
        let response = |rows| v1::StreamViewerRowsResponse {
            identity: Some(v1::ViewerViewIdentity::default()),
            sequence: 7,
            event: Some(v1::stream_viewer_rows_response::Event::UnifiedRows(
                v1::ViewerUnifiedRows {
                    start_row: 0,
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
