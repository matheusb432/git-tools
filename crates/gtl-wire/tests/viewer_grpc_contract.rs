use gtl_models::{
    diffs::ExcludedExtensions,
    paths::ProjectName,
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
        RenderHistoryId, ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId,
        ViewerVersion,
    },
};
use gtl_wire::{
    proto::viewer::{
        ViewerCodecError, decode_get_viewer_settings_response, decode_get_viewer_shell_response,
        decode_list_viewer_history_response, decode_stream_viewer_rows_response,
        encode_get_viewer_settings_response, encode_list_viewer_history_response,
        encode_viewer_shell, encode_viewer_unified_row, encode_viewer_view_identity,
    },
    v1::{
        self, StreamViewerRowsResponse, ViewerCodeLine, ViewerUnifiedRow, ViewerUnifiedRows,
        ViewerViewIdentity, stream_viewer_rows_response, viewer_unified_row,
    },
    viewer::{
        self, ViewerActiveState, ViewerCodeSpan, ViewerDiffDensity, ViewerDiffExclusions,
        ViewerDiffFileId, ViewerDiffLayout, ViewerFeedback, ViewerHistoryEntry, ViewerHistoryPage,
        ViewerPreferences, ViewerProjectDiffExclusions, ViewerRecipeKind, ViewerRowEvent,
        ViewerShell, ViewerSyntaxClass, ViewerTab, ViewerTabKind, ViewerTabState, ViewerTheme,
        ViewerUnifiedSourceRow, ViewerUserSettings,
    },
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
    let batch = match response.event {
        Some(stream_viewer_rows_response::Event::UnifiedRows(batch)) => Some(batch),
        _ => None,
    }
    .unwrap();
    assert_eq!(batch.file_id, "file-4");
    assert!(matches!(
        batch.rows[0].row,
        Some(viewer_unified_row::Row::Added(_))
    ));
}

#[test]
fn shell_codec_round_trips_the_process_neutral_contract() {
    let shell = ViewerShell {
        version: ViewerVersion::new(4),
        tabs: vec![ViewerTab {
            id: ViewerTabId::try_new(7).unwrap(),
            label: "git-tools".into(),
            kind: ViewerTabKind::Live,
            state: ViewerTabState::Ready,
        }],
        active: ViewerActiveState::Empty,
        preferences: ViewerPreferences {
            theme: ViewerTheme::Graphite,
            render_options: gtl_wire::viewer::ViewerRenderOptions {
                layout: ViewerDiffLayout::Split,
                density: ViewerDiffDensity::Full,
            },
        },
        feedback: Some(ViewerFeedback::TabClosed),
    };

    let encoded = encode_viewer_shell(shell.clone()).unwrap();
    let decoded = decode_get_viewer_shell_response(v1::GetViewerShellResponse {
        shell: Some(encoded),
    })
    .unwrap();

    assert_eq!(decoded, shell);
}

#[test]
fn streamed_row_codec_round_trips_utf8_span_boundaries() {
    let identity = viewer::ViewerViewIdentity {
        tab_id: ViewerTabId::try_new(7).unwrap(),
        range_generation: ViewerRangeGeneration::new(3),
        selection_generation: ViewerSelectionGeneration::new(2),
        render_options: gtl_wire::viewer::ViewerRenderOptions {
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        },
    };
    let file = ViewerDiffFileId::for_index(0);
    let row = viewer::ViewerUnifiedRow::Added(ViewerUnifiedSourceRow {
        old_line_number: None,
        new_line_number: Some(42),
        code: viewer::ViewerCodeLine {
            text: "let café = 42;".into(),
            spans: vec![
                ViewerCodeSpan {
                    byte_start: 0,
                    byte_end: 3,
                    syntax_class: Some(ViewerSyntaxClass::Keyword),
                    changed: false,
                },
                ViewerCodeSpan {
                    byte_start: 3,
                    byte_end: 15,
                    syntax_class: None,
                    changed: true,
                },
            ],
            long_line_character_count: None,
        },
    });
    let encoded = encode_viewer_unified_row(row.clone()).unwrap();
    let decoded = decode_stream_viewer_rows_response(v1::StreamViewerRowsResponse {
        identity: Some(encode_viewer_view_identity(identity)),
        sequence: 9,
        event: Some(stream_viewer_rows_response::Event::UnifiedRows(
            v1::ViewerUnifiedRows {
                file_id: file.as_str().to_owned(),
                rows: vec![encoded],
            },
        )),
    })
    .unwrap();

    assert_eq!(decoded.identity, identity);
    assert_eq!(decoded.sequence, 9);
    assert_eq!(
        decoded.event,
        ViewerRowEvent::UnifiedRows {
            file,
            rows: vec![row],
        }
    );
}

#[test]
fn streamed_row_codec_rejects_invalid_handwritten_span_ranges() {
    for spans in [
        vec![ViewerCodeSpan {
            byte_start: 1,
            byte_end: 5,
            syntax_class: None,
            changed: false,
        }],
        vec![ViewerCodeSpan {
            byte_start: 0,
            byte_end: 4,
            syntax_class: None,
            changed: false,
        }],
    ] {
        let row = viewer::ViewerUnifiedRow::Added(ViewerUnifiedSourceRow {
            old_line_number: None,
            new_line_number: Some(1),
            code: viewer::ViewerCodeLine {
                text: "café".into(),
                spans,
                long_line_character_count: None,
            },
        });

        assert_eq!(
            encode_viewer_unified_row(row),
            Err(ViewerCodecError::Unrepresentable)
        );
    }
}

#[test]
fn missing_required_shell_is_an_invalid_message() {
    assert_eq!(
        decode_get_viewer_shell_response(v1::GetViewerShellResponse { shell: None }),
        Err(ViewerCodecError::InvalidMessage)
    );
}

#[test]
fn history_page_codec_round_trips_navigation_and_identity() {
    let page = ViewerHistoryPage {
        entries: vec![ViewerHistoryEntry {
            id: RenderHistoryId::try_new(11).unwrap(),
            title: "git-tools · unpushed".into(),
            repository_name: ProjectName::try_new("git-tools").unwrap(),
            kind: ViewerRecipeKind::Diff,
            range_label: "origin/main..HEAD".into(),
            rendered_at: MachineTimestamp::try_from("2026-08-25T12:00:00Z").unwrap(),
        }],
        total_count: HistoryRenderCount::new(4),
        position: HistoryPagePosition::Page(
            HistoryPage::new(
                HistoryPageNumber::try_new(2).unwrap(),
                HistoryPageCount::try_new(4).unwrap(),
            )
            .unwrap(),
        ),
        has_newer: true,
        has_older: true,
    };

    let encoded = encode_list_viewer_history_response(page.clone()).unwrap();
    let decoded = decode_list_viewer_history_response(encoded).unwrap();

    assert_eq!(decoded, page);
}

#[test]
fn settings_codec_round_trips_exclusions_and_effective_values() {
    let settings = ViewerUserSettings {
        configuration_path: Some("/home/dev/.config/git-tools.toml".into()),
        configured_theme: Some(ViewerTheme::Hearth),
        effective_theme: ViewerTheme::Hearth,
        render_options: gtl_wire::viewer::ViewerRenderOptions {
            layout: ViewerDiffLayout::Split,
            density: ViewerDiffDensity::Full,
        },
        push_confirmation_required: true,
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: ExcludedExtensions::new(["lock"]),
            projects: vec![ViewerProjectDiffExclusions {
                project_name: ProjectName::try_new("git-tools").unwrap(),
                extensions: ExcludedExtensions::new(["snap"]),
            }],
        },
    };

    let encoded = encode_get_viewer_settings_response(settings.clone());
    let decoded = decode_get_viewer_settings_response(encoded).unwrap();

    assert_eq!(decoded, settings);
}
