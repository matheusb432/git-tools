use std::collections::BTreeSet;

use gtl_wire::v1::{
    self, StreamViewerRowsResponse, ViewerCodeLine, ViewerSplitCell, ViewerSplitRow,
    ViewerUnifiedRow, stream_viewer_rows_response, viewer_split_row, viewer_unified_row,
};
use prost::Message as _;
use sha2::{Digest, Sha256};

use super::StreamEvidence;

#[derive(Debug, thiserror::Error)]
pub enum StreamValidationError {
    #[error("viewer row stream sequence is {actual}, expected {expected}")]
    Sequence { actual: u64, expected: u64 },
    #[error("viewer row stream identity changed")]
    IdentityChanged,
    #[error("viewer row stream event is missing")]
    MissingEvent,
    #[error("viewer row stream file lifecycle is invalid: {reason}")]
    FileLifecycle { reason: String },
    #[error("viewer row stream reported file failure for {file_id}: {message}")]
    FileFailure { file_id: String, message: String },
    #[error("viewer row stream code line is invalid: {reason}")]
    InvalidCodeLine { reason: String },
    #[error("viewer row stream ended before all files finished")]
    Incomplete,
}

pub struct StreamValidator {
    digest: Sha256,
    identity: Option<v1::ViewerViewIdentity>,
    expected_sequence: u64,
    open_file: Option<String>,
    finished_files: BTreeSet<String>,
    message_count: usize,
    row_count: usize,
    encoded_bytes: usize,
}

impl StreamValidator {
    #[must_use]
    pub fn new() -> Self {
        Self {
            digest: Sha256::new(),
            identity: None,
            expected_sequence: 0,
            open_file: None,
            finished_files: BTreeSet::new(),
            message_count: 0,
            row_count: 0,
            encoded_bytes: 0,
        }
    }

    pub fn observe(
        &mut self,
        response: &StreamViewerRowsResponse,
    ) -> Result<(), StreamValidationError> {
        if response.sequence != self.expected_sequence {
            return Err(StreamValidationError::Sequence {
                actual: response.sequence,
                expected: self.expected_sequence,
            });
        }
        self.expected_sequence = self.expected_sequence.saturating_add(1);
        match (&self.identity, &response.identity) {
            (None, Some(identity)) => self.identity = Some(*identity),
            (Some(expected), Some(actual)) if expected == actual => {}
            (None | Some(_), None) | (Some(_), Some(_)) => {
                return Err(StreamValidationError::IdentityChanged);
            }
        }
        let event = response
            .event
            .as_ref()
            .ok_or(StreamValidationError::MissingEvent)?;
        self.validate_event(event)?;

        self.hash_event(event);
        self.message_count = self.message_count.saturating_add(1);
        self.encoded_bytes = self.encoded_bytes.saturating_add(response.encoded_len());
        Ok(())
    }

    fn hash_event(&mut self, event: &stream_viewer_rows_response::Event) {
        match event {
            stream_viewer_rows_response::Event::FileStarted(started) => {
                self.hash_value(0, started.file_id.as_bytes());
            }
            stream_viewer_rows_response::Event::UnifiedRows(batch) => {
                for row in &batch.rows {
                    self.hash_value(1, &row.encode_to_vec());
                }
            }
            stream_viewer_rows_response::Event::SplitRows(batch) => {
                for row in &batch.rows {
                    self.hash_value(2, &row.encode_to_vec());
                }
            }
            stream_viewer_rows_response::Event::FileFinished(finished) => {
                self.hash_value(3, finished.file_id.as_bytes());
                self.digest
                    .update(finished.line_number_digits.to_le_bytes());
            }
            stream_viewer_rows_response::Event::FileFailed(_) => {}
        }
    }

    fn hash_value(&mut self, kind: u8, value: &[u8]) {
        self.digest.update([kind]);
        self.digest.update((value.len() as u64).to_le_bytes());
        self.digest.update(value);
    }

    pub fn finish(self) -> Result<StreamEvidence, StreamValidationError> {
        if self.message_count == 0
            || self.row_count == 0
            || self.open_file.is_some()
            || self.finished_files.is_empty()
        {
            return Err(StreamValidationError::Incomplete);
        }
        Ok(StreamEvidence {
            message_count: self.message_count,
            row_count: self.row_count,
            encoded_bytes: self.encoded_bytes,
            semantic_sha256: hex_bytes(&self.digest.finalize()),
        })
    }

    fn validate_event(
        &mut self,
        event: &stream_viewer_rows_response::Event,
    ) -> Result<(), StreamValidationError> {
        match event {
            stream_viewer_rows_response::Event::FileStarted(started) => {
                self.validate_file_started(started)
            }
            stream_viewer_rows_response::Event::UnifiedRows(batch) => {
                self.validate_unified_rows(batch)
            }
            stream_viewer_rows_response::Event::SplitRows(batch) => self.validate_split_rows(batch),
            stream_viewer_rows_response::Event::FileFinished(finished) => {
                self.validate_file_finished(finished)
            }
            stream_viewer_rows_response::Event::FileFailed(failed) => {
                Err(StreamValidationError::FileFailure {
                    file_id: failed.file_id.clone(),
                    message: failed
                        .failure
                        .clone()
                        .and_then(gtl_wire::proto::failure::decode_failure)
                        .map_or_else(
                            || "unknown failure".to_owned(),
                            |failure| failure.to_string(),
                        ),
                })
            }
        }
    }

    fn validate_file_started(
        &mut self,
        started: &v1::ViewerFileStarted,
    ) -> Result<(), StreamValidationError> {
        if started.file_id.is_empty()
            || self.open_file.is_some()
            || self.finished_files.contains(&started.file_id)
        {
            return Err(file_lifecycle("file-started is duplicate or nested"));
        }
        self.open_file = Some(started.file_id.clone());
        Ok(())
    }

    fn validate_unified_rows(
        &mut self,
        batch: &v1::ViewerUnifiedRows,
    ) -> Result<(), StreamValidationError> {
        self.require_open_file(&batch.file_id)?;
        for row in &batch.rows {
            validate_unified_row(row)?;
        }
        self.row_count = self.row_count.saturating_add(batch.rows.len());
        Ok(())
    }

    fn validate_split_rows(
        &mut self,
        batch: &v1::ViewerSplitRows,
    ) -> Result<(), StreamValidationError> {
        self.require_open_file(&batch.file_id)?;
        for row in &batch.rows {
            validate_split_row(row)?;
        }
        self.row_count = self.row_count.saturating_add(batch.rows.len());
        Ok(())
    }

    fn validate_file_finished(
        &mut self,
        finished: &v1::ViewerFileFinished,
    ) -> Result<(), StreamValidationError> {
        self.require_open_file(&finished.file_id)?;
        self.open_file = None;
        self.finished_files.insert(finished.file_id.clone());
        Ok(())
    }

    fn require_open_file(&self, file_id: &str) -> Result<(), StreamValidationError> {
        if self.open_file.as_deref() == Some(file_id) {
            Ok(())
        } else {
            Err(file_lifecycle(format!(
                "event for {file_id} does not match the open file"
            )))
        }
    }
}

impl Default for StreamValidator {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_unified_row(row: &ViewerUnifiedRow) -> Result<(), StreamValidationError> {
    match row.row.as_ref() {
        Some(
            viewer_unified_row::Row::Context(source)
            | viewer_unified_row::Row::Added(source)
            | viewer_unified_row::Row::Removed(source),
        ) => validate_optional_code(source.code.as_ref()),
        Some(viewer_unified_row::Row::Meta(_) | viewer_unified_row::Row::Hunk(_)) => Ok(()),
        None => Err(invalid_code_line("unified row has no value")),
    }
}

fn validate_split_row(row: &ViewerSplitRow) -> Result<(), StreamValidationError> {
    match row.row.as_ref() {
        Some(viewer_split_row::Row::Context(context)) => {
            validate_optional_code(context.code.as_ref())
        }
        Some(viewer_split_row::Row::Pair(pair)) => {
            validate_optional_cell(pair.old.as_ref())?;
            validate_optional_cell(pair.new.as_ref())
        }
        Some(viewer_split_row::Row::Meta(_) | viewer_split_row::Row::Hunk(_)) => Ok(()),
        None => Err(invalid_code_line("split row has no value")),
    }
}

fn validate_optional_cell(cell: Option<&ViewerSplitCell>) -> Result<(), StreamValidationError> {
    cell.map_or(Ok(()), |cell| validate_optional_code(cell.code.as_ref()))
}

fn validate_optional_code(code: Option<&ViewerCodeLine>) -> Result<(), StreamValidationError> {
    let code = code.ok_or_else(|| invalid_code_line("source row has no code line"))?;
    let mut previous_end = 0_usize;
    for span in &code.spans {
        let start = span.byte_start as usize;
        let end = span.byte_end as usize;
        if start < previous_end
            || start > end
            || end > code.text.len()
            || !code.text.is_char_boundary(start)
            || !code.text.is_char_boundary(end)
        {
            return Err(invalid_code_line(format!(
                "span {start}..{end} is outside a {}-byte line",
                code.text.len()
            )));
        }
        previous_end = end;
    }
    Ok(())
}

fn file_lifecycle(reason: impl Into<String>) -> StreamValidationError {
    StreamValidationError::FileLifecycle {
        reason: reason.into(),
    }
}

fn invalid_code_line(reason: impl Into<String>) -> StreamValidationError {
    StreamValidationError::InvalidCodeLine {
        reason: reason.into(),
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_hashes_complete_file_streams_deterministically() {
        let first = evidence(valid_stream()).unwrap();
        let second = evidence(valid_stream()).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.message_count, 3);
        assert_eq!(first.row_count, 1);
        assert_eq!(first.semantic_sha256.len(), 64);
    }

    #[test]
    fn semantic_digest_ignores_batch_boundaries_but_preserves_rows() {
        let mut split = valid_stream();
        split.insert(2, split[1].clone());
        for (sequence, message) in split.iter_mut().enumerate() {
            message.sequence = sequence as u64;
        }
        let mut combined = valid_stream();
        let batch = match combined[1].event.as_mut() {
            Some(stream_viewer_rows_response::Event::UnifiedRows(batch)) => Some(batch),
            _ => None,
        }
        .unwrap();
        batch.rows.push(batch.rows[0].clone());
        for messages in [&mut split, &mut combined] {
            if let Some(stream_viewer_rows_response::Event::FileStarted(started)) =
                messages[0].event.as_mut()
            {
                started.row_count = 2;
            }
            if let Some(stream_viewer_rows_response::Event::FileFinished(finished)) =
                messages.last_mut().unwrap().event.as_mut()
            {
                finished.end_row = 2;
            }
        }
        if let Some(stream_viewer_rows_response::Event::UnifiedRows(batch)) =
            split[2].event.as_mut()
        {
            batch.start_row = 1;
        }
        let first = evidence(combined).unwrap();
        let second = evidence(split).unwrap();
        assert_eq!(first.row_count, second.row_count);
        assert_ne!(first.message_count, second.message_count);
        assert_eq!(first.semantic_sha256, second.semantic_sha256);
        assert_ne!(
            first.semantic_sha256,
            evidence(valid_stream()).unwrap().semantic_sha256
        );
    }

    #[test]
    fn validator_rejects_non_utf8_span_boundaries() {
        let mut messages = valid_stream();
        let batch = match messages[1].event.as_mut() {
            Some(stream_viewer_rows_response::Event::UnifiedRows(batch)) => Some(batch),
            _ => None,
        }
        .unwrap();
        let source = match batch.rows[0].row.as_mut() {
            Some(viewer_unified_row::Row::Added(source)) => Some(source),
            _ => None,
        }
        .unwrap();
        source.code.as_mut().unwrap().spans[0].byte_end = 1;

        let error = evidence(messages).unwrap_err();
        assert!(error.to_string().contains("outside a 2-byte line"));
    }

    fn evidence(
        messages: Vec<StreamViewerRowsResponse>,
    ) -> Result<StreamEvidence, StreamValidationError> {
        let mut validator = StreamValidator::new();
        for message in messages {
            validator.observe(&message)?;
        }
        validator.finish()
    }

    fn valid_stream() -> Vec<StreamViewerRowsResponse> {
        let identity = v1::ViewerViewIdentity {
            tab_id: 1,
            range_generation: 2,
            selection_generation: 3,
            render_options: None,
        };
        vec![
            StreamViewerRowsResponse {
                identity: Some(identity),
                sequence: 0,
                event: Some(stream_viewer_rows_response::Event::FileStarted(
                    v1::ViewerFileStarted {
                        file_id: "f-0".to_owned(),
                        start_row: 0,
                        row_count: 1,
                    },
                )),
            },
            StreamViewerRowsResponse {
                identity: Some(identity),
                sequence: 1,
                event: Some(stream_viewer_rows_response::Event::UnifiedRows(
                    v1::ViewerUnifiedRows {
                        file_id: "f-0".to_owned(),
                        start_row: 0,
                        rows: vec![ViewerUnifiedRow {
                            row: Some(viewer_unified_row::Row::Added(v1::ViewerUnifiedSourceRow {
                                old_line_number: None,
                                new_line_number: Some(1),
                                code: Some(ViewerCodeLine {
                                    text: "é".to_owned(),
                                    spans: vec![v1::ViewerCodeSpan {
                                        byte_start: 0,
                                        byte_end: 2,
                                        syntax_class: 0,
                                        changed: true,
                                    }],
                                    omitted_character_count: None,
                                }),
                            })),
                        }],
                    },
                )),
            },
            StreamViewerRowsResponse {
                identity: Some(identity),
                sequence: 2,
                event: Some(stream_viewer_rows_response::Event::FileFinished(
                    v1::ViewerFileFinished {
                        file_id: "f-0".to_owned(),
                        line_number_digits: 1,
                        end_row: 1,
                    },
                )),
            },
        ]
    }
}
