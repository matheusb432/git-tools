use gtl_wire::v1::{
    StreamViewerRowsResponse, ViewerCodeLine, ViewerUnifiedRow, ViewerUnifiedRows,
    ViewerViewIdentity, stream_viewer_rows_response, viewer_unified_row,
};

#[test]
fn row_stream_events_carry_identity_sequence_file_and_typed_rows() {
    let identity = ViewerViewIdentity {
        tab_id: 7,
        range_generation: 3,
        selection_generation: 2,
        render_options: None,
    };
    let response = StreamViewerRowsResponse {
        identity: Some(identity),
        sequence: 11,
        event: Some(stream_viewer_rows_response::Event::UnifiedRows(
            ViewerUnifiedRows {
                file_id: "file-4".to_owned(),
                rows: vec![ViewerUnifiedRow {
                    row: Some(viewer_unified_row::Row::Added(
                        gtl_wire::v1::ViewerUnifiedSourceRow {
                            old_line_number: None,
                            new_line_number: Some(42),
                            code: Some(ViewerCodeLine {
                                text: "let answer = 42;".to_owned(),
                                spans: Vec::new(),
                                long_line_character_count: None,
                            }),
                        },
                    )),
                }],
            },
        )),
    };

    assert_eq!(response.identity, Some(identity));
    assert_eq!(response.sequence, 11);
    let Some(stream_viewer_rows_response::Event::UnifiedRows(batch)) = response.event else {
        panic!("unified row batch expected");
    };
    assert_eq!(batch.file_id, "file-4");
    assert!(matches!(
        batch.rows[0].row,
        Some(viewer_unified_row::Row::Added(_))
    ));
}
