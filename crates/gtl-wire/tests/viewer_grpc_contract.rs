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
        ViewerActiveState, ViewerCodeLine as ContractCodeLine, ViewerCodeSpan, ViewerDiffDensity,
        ViewerDiffExclusions, ViewerDiffFileId, ViewerDiffLayout, ViewerFeedback,
        ViewerHistoryEntry, ViewerHistoryPage, ViewerPreferences, ViewerProjectDiffExclusions,
        ViewerRecipeKind, ViewerRowEvent, ViewerShell, ViewerSyntaxClass, ViewerTab, ViewerTabKind,
        ViewerTabState, ViewerTheme, ViewerUnifiedRow as ContractUnifiedRow,
        ViewerUnifiedSourceRow, ViewerUserSettings, ViewerViewIdentity as ContractViewIdentity,
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
    let Some(stream_viewer_rows_response::Event::UnifiedRows(batch)) = response.event else {
        panic!("unified row batch expected");
    };
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
            id: ViewerTabId::try_new(7).expect("positive tab ID"),
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

    let encoded = encode_viewer_shell(shell.clone()).expect("shell encodes");
    let decoded = decode_get_viewer_shell_response(v1::GetViewerShellResponse {
        shell: Some(encoded),
    })
    .expect("shell decodes");

    assert_eq!(decoded, shell);
}

#[test]
fn streamed_row_codec_round_trips_utf8_span_boundaries() {
    let identity = ContractViewIdentity {
        tab_id: ViewerTabId::try_new(7).expect("positive tab ID"),
        range_generation: ViewerRangeGeneration::new(3),
        selection_generation: ViewerSelectionGeneration::new(2),
        render_options: gtl_wire::viewer::ViewerRenderOptions {
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        },
    };
    let file = ViewerDiffFileId::for_index(0);
    let row = ContractUnifiedRow::Added(ViewerUnifiedSourceRow {
        old_line_number: None,
        new_line_number: Some(42),
        code: ContractCodeLine {
            text: "let café = 42;".into(),
            spans: vec![
                ViewerCodeSpan {
                    text: "let".into(),
                    syntax_class: Some(ViewerSyntaxClass::Keyword),
                    changed: false,
                },
                ViewerCodeSpan {
                    text: " café = 42;".into(),
                    syntax_class: None,
                    changed: true,
                },
            ],
            long_line_character_count: None,
        },
    });
    let encoded = encode_viewer_unified_row(row.clone()).expect("row encodes");
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
    .expect("row stream item decodes");

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
            id: RenderHistoryId::try_new(11).expect("positive history ID"),
            title: "git-tools · unpushed".into(),
            repository_name: ProjectName::try_new("git-tools").expect("project name"),
            kind: ViewerRecipeKind::Diff,
            range_label: "origin/main..HEAD".into(),
            rendered_at: MachineTimestamp::try_from("2026-08-25T12:00:00Z")
                .expect("machine timestamp"),
        }],
        total_count: HistoryRenderCount::new(4),
        position: HistoryPagePosition::Page(
            HistoryPage::new(
                HistoryPageNumber::try_new(2).expect("positive page number"),
                HistoryPageCount::try_new(4).expect("positive page count"),
            )
            .expect("page is within the history"),
        ),
        has_newer: true,
        has_older: true,
    };

    let encoded = encode_list_viewer_history_response(page.clone()).expect("history page encodes");
    let decoded = decode_list_viewer_history_response(encoded).expect("history page decodes");

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
                project_name: ProjectName::try_new("git-tools").expect("project name"),
                extensions: ExcludedExtensions::new(["snap"]),
            }],
        },
    };

    let encoded = encode_get_viewer_settings_response(settings.clone());
    let decoded = decode_get_viewer_settings_response(encoded).expect("settings decode");

    assert_eq!(decoded, settings);
}
