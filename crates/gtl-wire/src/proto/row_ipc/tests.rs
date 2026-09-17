use super::*;
use crate::v1::stream_viewer_rows_response::Event;

#[test]
fn binary_frames_preserve_rows_offsets_identity_and_terminal_events() {
    let responses = [
        response(
            0,
            Event::FileStarted(v1::ViewerFileStarted {
                file_id: "file-2".to_owned(),
                row_count: 20_005,
                start_row: 19_968,
            }),
        ),
        response(
            1,
            Event::UnifiedRows(v1::ViewerUnifiedRows {
                file_id: "file-2".to_owned(),
                start_row: 19_968,
                rows: vec![v1::ViewerUnifiedRow {
                    row: Some(v1::viewer_unified_row::Row::Added(
                        v1::ViewerUnifiedSourceRow {
                            old_line_number: None,
                            new_line_number: Some(19_965),
                            code: Some(v1::ViewerCodeLine {
                                text: "λ\0\t\n\"quoted\"".to_owned(),
                                spans: Vec::new(),
                                omitted_character_count: None,
                            }),
                        },
                    )),
                }],
            }),
        ),
        response(
            2,
            Event::SplitRows(v1::ViewerSplitRows {
                file_id: "file-2".to_owned(),
                start_row: 19_968,
                rows: vec![v1::ViewerSplitRow {
                    row: Some(v1::viewer_split_row::Row::Hunk("@@ -1 +1 @@".to_owned())),
                }],
            }),
        ),
        response(
            3,
            Event::FileFinished(v1::ViewerFileFinished {
                file_id: "file-2".to_owned(),
                line_number_digits: 5,
                end_row: 19_969,
            }),
        ),
    ];
    let expected = responses
        .iter()
        .cloned()
        .map(decode_stream_viewer_rows_response)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let frames = responses
        .iter()
        .map(encode_frame)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let bytes = encode_batch(&frames).unwrap();
    assert_eq!(decode_batch(&bytes).unwrap(), expected);
    assert!(
        decode_batch(&encode_batch(&[]).unwrap())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn truncated_extra_and_incompatible_frames_are_rejected() {
    let frame = encode_frame(&response(
        0,
        Event::FileFailed(v1::ViewerFileFailed {
            file_id: "file-0".to_owned(),
            code: v1::ViewerFileFailureCode::SourceUnavailable.into(),
            message: "source changed".to_owned(),
            retryable: true,
        }),
    ))
    .unwrap();
    let bytes = encode_batch(&[frame]).unwrap();
    for length in 0..bytes.len() {
        assert_eq!(
            decode_batch(&bytes[..length]),
            Err(ViewerCodecError::InvalidMessage)
        );
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert_eq!(decode_batch(&extra), Err(ViewerCodecError::InvalidMessage));
    let mut version = bytes.clone();
    version[..4].copy_from_slice(&(VIEWER_PROTOCOL_VERSION + 1).to_le_bytes());
    assert_eq!(
        decode_batch(&version),
        Err(ViewerCodecError::InvalidMessage)
    );
    let mut count = bytes.clone();
    count[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode_batch(&count), Err(ViewerCodecError::InvalidMessage));
    let mut length = bytes;
    length[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(decode_batch(&length), Err(ViewerCodecError::InvalidMessage));
}

#[test]
fn malformed_protobuf_and_invalid_contract_values_are_rejected() {
    let malformed = encode_batch(&[vec![0xff]]).unwrap();
    assert_eq!(
        decode_batch(&malformed),
        Err(ViewerCodecError::InvalidMessage)
    );
    let invalid =
        encode_batch(&[encode_frame(&v1::StreamViewerRowsResponse::default()).unwrap()]).unwrap();
    assert_eq!(
        decode_batch(&invalid),
        Err(ViewerCodecError::InvalidMessage)
    );
    assert_eq!(
        encode_batch(&vec![Vec::new(); ROW_IPC_BATCH_ITEMS_MAX + 1]),
        Err(ViewerCodecError::Unrepresentable)
    );
    assert_eq!(
        encode_batch(&[vec![0; VIEWER_ROW_MAX_ENCODED_BYTES + 1]]),
        Err(ViewerCodecError::Unrepresentable)
    );
}

fn response(sequence: u64, event: Event) -> v1::StreamViewerRowsResponse {
    v1::StreamViewerRowsResponse {
        identity: Some(v1::ViewerViewIdentity {
            tab_id: 7,
            range_generation: 8,
            selection_generation: 9,
            render_options: Some(v1::ViewerRenderOptions {
                wrap_lines: false,
                layout: v1::ViewerDiffLayout::Unified.into(),
                density: v1::ViewerDiffDensity::Compact.into(),
            }),
        }),
        sequence,
        event: Some(event),
    }
}
