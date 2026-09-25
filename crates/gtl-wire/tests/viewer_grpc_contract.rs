use std::num::NonZeroU32;

use gtl_models::{
    diffs::{CommitId, DiffViewTitle, ExtensionFilter, ExtensionFilterMode, FileExtensions},
    git::{CommitCount, GitHead, GitRange, GitRevision},
    paths::ProjectName,
    recipes::{RecipeLabel, RecipeLabelChanges},
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
        RenderHistoryId, ViewerKeybindingAction, ViewerKeybindingPlatform, ViewerKeybindings,
        ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId, ViewerTabPlacement,
        ViewerVersion,
    },
};
use gtl_wire::{
    proto::viewer::{
        ViewerCodecError, decode_edit_settings_request, decode_find_viewer_diff_request,
        decode_find_viewer_diff_response, decode_get_viewer_settings_response,
        decode_get_viewer_shell_response, decode_list_viewer_history_response,
        decode_move_viewer_tab_request, decode_search_viewer_files_request,
        decode_search_viewer_files_response, decode_stream_viewer_rows_response,
        encode_edit_settings_request, encode_find_viewer_diff_request,
        encode_find_viewer_diff_response, encode_get_viewer_settings_response,
        encode_list_viewer_history_response, encode_move_viewer_tab_request,
        encode_search_viewer_files_request, encode_search_viewer_files_response,
        encode_viewer_shell, encode_viewer_unified_row, encode_viewer_view_identity,
    },
    v1::{
        self, StreamViewerRowsResponse, ViewerCodeLine, ViewerUnifiedRow, ViewerUnifiedRows,
        ViewerViewIdentity, stream_viewer_rows_response, viewer_unified_row,
    },
    viewer::{
        self, EditSettingsRequest, FieldUpdate, FindViewerDiff, MoveViewerTab, SearchViewerFiles,
        ViewerActiveState, ViewerCodeSpan, ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout,
        ViewerDiffSearchDirection, ViewerDiffSearchMatch, ViewerDiffSearchResult, ViewerFeedback,
        ViewerFileSearchResult, ViewerHistoryEntry, ViewerHistoryPage, ViewerPreferences,
        ViewerRecipeKind, ViewerRowEvent, ViewerShell, ViewerSyntaxClass, ViewerTab, ViewerTabKind,
        ViewerTabState, ViewerTheme, ViewerUnifiedSourceRow, ViewerUserSettings,
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
                start_row: 0,
                file_id: "file-4".to_owned(),
                rows: vec![ViewerUnifiedRow {
                    row: Some(viewer_unified_row::Row::Added(
                        gtl_wire::v1::ViewerUnifiedSourceRow {
                            old_line_number: None,
                            new_line_number: Some(42),
                            code: Some(ViewerCodeLine {
                                text: "let answer = 42;".to_owned(),
                                spans: Vec::new(),
                                omitted_character_count: None,
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

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn changes_label(changes: RecipeLabelChanges) -> TestResult<RecipeLabel> {
    Ok(RecipeLabel::Changes {
        repository: ProjectName::try_new("git-tools")?,
        changes,
    })
}

/// One label per variant, so each protobuf `oneof` case is exercised.
fn every_recipe_label() -> TestResult<Vec<RecipeLabel>> {
    Ok(vec![
        RecipeLabel::Named {
            name: ProjectName::try_new("Release review")?,
        },
        RecipeLabel::Repository {
            repository: ProjectName::try_new("git-tools")?,
        },
        changes_label(RecipeLabelChanges::Unpushed)?,
        changes_label(RecipeLabelChanges::UnpushedCommits {
            count: CommitCount::new(3),
        })?,
        changes_label(RecipeLabelChanges::WorkingTree {
            base: GitRevision::try_new("HEAD~2")?,
        })?,
        changes_label(RecipeLabelChanges::Range {
            range: GitRange::try_new("v1..v2")?,
        })?,
        changes_label(RecipeLabelChanges::MergeInto {
            base: GitRevision::try_new("main")?,
        })?,
        changes_label(RecipeLabelChanges::Merge {
            branch: GitHead::try_from("feature".to_owned())?,
            upstream: GitRevision::try_new("origin/main")?,
        })?,
        changes_label(RecipeLabelChanges::LastCommits {
            count: NonZeroU32::MIN.saturating_add(4),
        })?,
    ])
}

fn tab(id: u64, label: RecipeLabel) -> TestResult<ViewerTab> {
    Ok(ViewerTab {
        custom_name: None,
        pinned: false,
        id: ViewerTabId::try_new(id)?,
        label,
        kind: ViewerTabKind::Live,
        state: ViewerTabState::Ready,
    })
}

#[test]
fn shell_codec_round_trips_the_process_neutral_contract() -> TestResult {
    let labels = every_recipe_label()?;
    let shell = ViewerShell {
        version: ViewerVersion::new(4),
        focus_request_version: Some(ViewerVersion::new(3)),
        tabs: labels
            .iter()
            .cloned()
            .zip(7..)
            .map(|(label, id)| tab(id, label))
            .collect::<TestResult<_>>()?,
        active: ViewerActiveState::Empty,
        preferences: ViewerPreferences {
            accessibility: gtl_models::settings::ViewerAccessibility::default(),
            language: gtl_models::settings::ViewerLanguage::PtBr,
            date_format: gtl_models::settings::ViewerDateFormat::Relative,
            sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
            theme: ViewerTheme::Dark,
            render_options: gtl_wire::viewer::ViewerRenderOptions {
                wrap_lines: true,
                layout: ViewerDiffLayout::Split,
                density: ViewerDiffDensity::Full,
            },
            keybindings: ViewerKeybindings::try_from_fn(
                ViewerKeybindingPlatform::Linux,
                |action| match action {
                    ViewerKeybindingAction::SearchFiles => "alt+p".parse().unwrap(),
                    ViewerKeybindingAction::SearchTextInAllFiles => "ctrl+shift+f".parse().unwrap(),
                    ViewerKeybindingAction::ToggleFilesSidebar
                    | ViewerKeybindingAction::ToggleCommitsSidebar => {
                        ViewerKeybindings::default()[action]
                    }
                },
            )
            .unwrap(),
        },
        feedback: Some(ViewerFeedback::SnapshotRecipesSkipped { labels }),
    };

    let encoded = encode_viewer_shell(shell.clone())?;
    let decoded = decode_get_viewer_shell_response(v1::GetViewerShellResponse {
        shell: Some(encoded),
    })?;

    assert_eq!(decoded, shell);
    Ok(())
}

#[test]
fn shell_decoding_rejects_missing_or_invalid_recipe_label_parts() -> TestResult {
    let shell = ViewerShell {
        version: ViewerVersion::new(1),
        focus_request_version: None,
        tabs: vec![tab(
            7,
            changes_label(RecipeLabelChanges::LastCommits {
                count: NonZeroU32::MIN,
            })?,
        )?],
        active: ViewerActiveState::Empty,
        preferences: ViewerPreferences {
            accessibility: gtl_models::settings::ViewerAccessibility::default(),
            language: gtl_models::settings::ViewerLanguage::default(),
            date_format: gtl_models::settings::ViewerDateFormat::default(),
            sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
            theme: ViewerTheme::Dark,
            render_options: gtl_wire::viewer::ViewerRenderOptions {
                wrap_lines: false,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
            },
            keybindings: ViewerKeybindings::default(),
        },
        feedback: None,
    };
    let encoded = encode_viewer_shell(shell)?;
    let mut empty_feedback = encoded.clone();
    empty_feedback.feedback = Some(v1::ViewerFeedback {
        kind: v1::ViewerFeedbackKind::SnapshotRecipesSkipped as i32,
        recipe_labels: Vec::new(),
    });
    assert_eq!(
        decode_get_viewer_shell_response(v1::GetViewerShellResponse {
            shell: Some(empty_feedback),
        }),
        Err(ViewerCodecError::InvalidMessage),
        "skipped feedback names at least one snapshot"
    );
    let decode_with_label = |label: Option<v1::ViewerRecipeLabel>| {
        let mut shell = encoded.clone();
        shell.tabs[0].recipe_label = label;
        decode_get_viewer_shell_response(v1::GetViewerShellResponse { shell: Some(shell) })
    };
    let changes = |changes| v1::ViewerRecipeLabel {
        label: Some(v1::viewer_recipe_label::Label::Changes(
            v1::ViewerRecipeChangesLabel {
                repository: "git-tools".to_owned(),
                changes,
            },
        )),
    };

    assert!(decode_with_label(encoded.tabs[0].recipe_label.clone()).is_ok());
    for label in [
        None,
        Some(v1::ViewerRecipeLabel { label: None }),
        Some(v1::ViewerRecipeLabel {
            label: Some(v1::viewer_recipe_label::Label::Name(String::new())),
        }),
        Some(changes(None)),
        Some(changes(Some(
            v1::viewer_recipe_changes_label::Changes::LastCommitCount(0),
        ))),
        Some(changes(Some(
            v1::viewer_recipe_changes_label::Changes::Merge(v1::ViewerRecipeMergeLabel {
                branch: "feature".to_owned(),
                upstream: " ".to_owned(),
            }),
        ))),
    ] {
        assert_eq!(
            decode_with_label(label.clone()),
            Err(ViewerCodecError::InvalidMessage),
            "{label:?}"
        );
    }
    Ok(())
}

fn raw_active_view() -> TestResult<v1::ViewerActiveView> {
    Ok(v1::ViewerActiveView {
        modified_files: false,
        row_source: v1::ViewerRowSourceState::Ready as i32,
        identity: Some(encode_viewer_view_identity(viewer_identity()?)),
        content_id: Some(vec![42; 32]),
        view_title: Some(v1::ViewerDiffViewTitle {
            title: Some(v1::viewer_diff_view_title::Title::Diff(v1::Empty {})),
        }),
        commit_count: 17,
        repository_name: "repo".into(),
        branch: "main".into(),
        upstream: "HEAD".into(),
        command: Some(v1::ViewerCommandLine::default()),
        commit_selection: Some(v1::ViewerCommitSelection {
            state: v1::ViewerCommitSelectionState::None as i32,
            ..Default::default()
        }),
        footer: Some(v1::ViewerFooter::default()),
        ..Default::default()
    })
}

fn encoded_ready_shell(active: v1::ViewerActiveView) -> TestResult<v1::ViewerShell> {
    let shell = ViewerShell {
        version: ViewerVersion::new(1),
        focus_request_version: None,
        tabs: Vec::new(),
        active: ViewerActiveState::Empty,
        preferences: ViewerPreferences {
            accessibility: gtl_models::settings::ViewerAccessibility::default(),
            language: gtl_models::settings::ViewerLanguage::PtBr,
            date_format: gtl_models::settings::ViewerDateFormat::Relative,
            sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
            theme: ViewerTheme::Dark,
            render_options: viewer_identity()?.render_options,
            keybindings: ViewerKeybindings::default(),
        },
        feedback: None,
    };
    let mut encoded = encode_viewer_shell(shell)?;
    encoded.active = Some(v1::ViewerActiveState {
        state: Some(v1::viewer_active_state::State::Ready(Box::new(
            v1::ViewerReadyState { view: Some(active) },
        ))),
    });
    Ok(encoded)
}

#[test]
fn active_view_titles_round_trip_and_reject_invalid_parts() -> TestResult {
    use v1::viewer_diff_view_title::Title;

    let decode_title =
        |title: Option<Title>| -> TestResult<Result<DiffViewTitle, ViewerCodecError>> {
            let shell = encoded_ready_shell(v1::ViewerActiveView {
                view_title: Some(v1::ViewerDiffViewTitle { title }),
                ..raw_active_view()?
            })?;
            Ok(
                decode_get_viewer_shell_response(v1::GetViewerShellResponse { shell: Some(shell) })
                    .map(|shell| match shell.active {
                        ViewerActiveState::Ready { view } => Some(view.title),
                        _ => None,
                    })
                    .and_then(|title| title.ok_or(ViewerCodecError::InvalidMessage)),
            )
        };
    let commit_id = "abcdef0123456789abcdef0123456789abcdef01";

    assert_eq!(
        decode_title(Some(Title::Diff(v1::Empty {})))?,
        Ok(DiffViewTitle::Diff)
    );
    assert_eq!(
        decode_title(Some(Title::MergeDiff(v1::Empty {})))?,
        Ok(DiffViewTitle::MergeDiff)
    );
    assert_eq!(
        decode_title(Some(Title::CommitId(commit_id.to_owned())))?,
        Ok(DiffViewTitle::Commit {
            id: CommitId::try_from(commit_id)?,
        })
    );
    assert_eq!(
        decode_title(Some(Title::Name("Release review".to_owned())))?,
        Ok(DiffViewTitle::Named {
            name: ProjectName::try_new("Release review")?,
        })
    );
    for title in [
        None,
        Some(Title::CommitId("abc1234".to_owned())),
        Some(Title::Name(String::new())),
    ] {
        assert_eq!(
            decode_title(title.clone())?,
            Err(ViewerCodecError::InvalidMessage),
            "{title:?}"
        );
    }
    Ok(())
}

#[test]
fn ready_shell_metadata_survives_protobuf_and_rejects_invalid_content_ids()
-> Result<(), Box<dyn std::error::Error>> {
    use prost::Message as _;
    let active = raw_active_view()?;
    let encoded = encoded_ready_shell(active.clone())?;
    let bytes = encoded.encode_to_vec();
    let decoded = decode_get_viewer_shell_response(v1::GetViewerShellResponse {
        shell: Some(v1::ViewerShell::decode(bytes.as_slice()).unwrap()),
    })
    .unwrap();
    let ViewerActiveState::Ready { view } = &decoded.active else {
        return Err("expected a ready view".into());
    };
    assert_eq!(
        view.content_id,
        viewer::ViewerRowContentId::from_digest([42; 32])
    );
    assert_eq!(view.commit_count, 17);
    assert!(view.commits.is_empty());
    assert_eq!(encode_viewer_shell(decoded).unwrap(), encoded);
    for content_id in [None, Some(vec![]), Some(vec![42; 31]), Some(vec![42; 33])] {
        let mut invalid = encoded.clone();
        invalid.active = Some(v1::ViewerActiveState {
            state: Some(v1::viewer_active_state::State::Ready(Box::new(
                v1::ViewerReadyState {
                    view: Some(v1::ViewerActiveView {
                        modified_files: false,
                        row_source: v1::ViewerRowSourceState::Ready as i32,
                        content_id,
                        ..active.clone()
                    }),
                },
            ))),
        });
        assert_eq!(
            decode_get_viewer_shell_response(v1::GetViewerShellResponse {
                shell: Some(invalid)
            }),
            Err(ViewerCodecError::InvalidMessage)
        );
    }
    Ok(())
}

#[test]
fn shell_codec_rejects_invalid_or_conflicting_keybindings() {
    for (search_files, search_text_in_all_files) in [("Shift+P", "Ctrl+F"), ("Ctrl+F", "ctrl+f")] {
        let response = v1::GetViewerShellResponse {
            shell: Some(v1::ViewerShell {
                version: 1,
                focus_request_version: None,
                tabs: Vec::new(),
                active: Some(v1::ViewerActiveState {
                    state: Some(v1::viewer_active_state::State::Empty(v1::Empty {})),
                }),
                preferences: Some(v1::ViewerPreferences {
                    language: v1::ViewerLanguage::EnUs as i32,
                    date_format: v1::ViewerDateFormat::Iso as i32,
                    accessibility: Some(v1::ViewerAccessibility {
                        ui_scale_percent: 100,
                        reduce_motion: false,
                    }),
                    sidebars: Some(v1::ViewerSidebarVisibility {
                        files: true,
                        commits: true,
                    }),
                    theme: v1::ViewerTheme::Dark as i32,
                    render_options: Some(v1::ViewerRenderOptions {
                        wrap_lines: false,
                        layout: v1::ViewerDiffLayout::Unified as i32,
                        density: v1::ViewerDiffDensity::Compact as i32,
                    }),
                    keybindings: Some(v1::ViewerKeybindings {
                        toggle_files_sidebar: "ctrl+b".to_owned(),
                        toggle_commits_sidebar: "ctrl+alt+b".to_owned(),
                        platform: v1::ViewerKeybindingPlatform::Linux as i32,
                        search_files: search_files.to_owned(),
                        search_text_in_all_files: search_text_in_all_files.to_owned(),
                    }),
                }),
                feedback: None,
            }),
        };

        assert_eq!(
            decode_get_viewer_shell_response(response),
            Err(ViewerCodecError::InvalidMessage)
        );
    }
}

#[test]
fn streamed_row_codec_round_trips_utf8_span_boundaries() {
    let identity = viewer::ViewerViewIdentity {
        tab_id: ViewerTabId::try_new(7).unwrap(),
        range_generation: ViewerRangeGeneration::new(3),
        selection_generation: ViewerSelectionGeneration::new(2),
        render_options: gtl_wire::viewer::ViewerRenderOptions {
            wrap_lines: false,
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
            omitted_character_count: None,
        },
    });
    let encoded = encode_viewer_unified_row(row.clone()).unwrap();
    let decoded = decode_stream_viewer_rows_response(v1::StreamViewerRowsResponse {
        identity: Some(encode_viewer_view_identity(identity)),
        sequence: 9,
        event: Some(stream_viewer_rows_response::Event::UnifiedRows(
            v1::ViewerUnifiedRows {
                start_row: 0,
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
            start_row: 0,
            file,
            rows: vec![row],
        }
    );
}

#[test]
fn viewer_file_search_codec_round_trips_identity_query_and_matches()
-> Result<(), Box<dyn std::error::Error>> {
    let identity = viewer_identity()?;
    let request = SearchViewerFiles {
        identity,
        query: "src/render".into(),
    };
    let result = ViewerFileSearchResult {
        identity,
        files: vec![
            ViewerDiffFileId::for_index(2),
            ViewerDiffFileId::for_index(7),
        ],
    };

    assert_eq!(
        decode_search_viewer_files_request(encode_search_viewer_files_request(request.clone()))
            .unwrap(),
        request
    );
    assert_eq!(
        decode_search_viewer_files_response(encode_search_viewer_files_response(result.clone()))
            .unwrap(),
        result
    );
    Ok(())
}

#[test]
fn viewer_diff_search_codec_round_trips_direction_anchor_and_result()
-> Result<(), Box<dyn std::error::Error>> {
    let identity = viewer_identity()?;
    let found = ViewerDiffSearchMatch {
        file: ViewerDiffFileId::for_index(3),
        row_index: 42,
    };
    let request = FindViewerDiff {
        identity,
        query: "needle".into(),
        direction: ViewerDiffSearchDirection::Backward,
        anchor: Some(found.clone()),
    };
    let result = ViewerDiffSearchResult {
        identity,
        total_matches: 9,
        active_match: Some(found),
        wrapped: true,
    };

    assert_eq!(
        decode_find_viewer_diff_request(encode_find_viewer_diff_request(request.clone())).unwrap(),
        request
    );
    assert_eq!(
        decode_find_viewer_diff_response(encode_find_viewer_diff_response(&result)).unwrap(),
        result
    );
    Ok(())
}

fn viewer_identity() -> Result<viewer::ViewerViewIdentity, Box<dyn std::error::Error>> {
    Ok(viewer::ViewerViewIdentity {
        tab_id: ViewerTabId::try_new(7)?,
        range_generation: ViewerRangeGeneration::new(3),
        selection_generation: ViewerSelectionGeneration::new(2),
        render_options: viewer::ViewerRenderOptions {
            wrap_lines: false,
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        },
    })
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
                omitted_character_count: None,
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
fn move_tab_codec_preserves_identity_and_rejects_unspecified_placement() {
    let request = MoveViewerTab {
        tab_id: ViewerTabId::try_new(3).unwrap(),
        target_tab_id: ViewerTabId::try_new(7).unwrap(),
        placement: ViewerTabPlacement::After,
    };
    let encoded = encode_move_viewer_tab_request(request);

    assert_eq!(encoded.tab_id, 3);
    assert_eq!(encoded.target_tab_id, 7);
    assert_eq!(encoded.placement, v1::ViewerTabPlacement::After as i32);
    assert_eq!(decode_move_viewer_tab_request(encoded), Ok(request));
    assert_eq!(
        decode_move_viewer_tab_request(v1::MoveViewerTabRequest {
            tab_id: 3,
            target_tab_id: 7,
            placement: v1::ViewerTabPlacement::Unspecified as i32,
        }),
        Err(ViewerCodecError::InvalidMessage)
    );
}

#[test]
fn history_page_codec_round_trips_navigation_and_identity() {
    let page = ViewerHistoryPage {
        projects: Vec::new(),
        entries: vec![ViewerHistoryEntry {
            id: RenderHistoryId::try_new(11).unwrap(),
            label: changes_label(RecipeLabelChanges::UnpushedCommits {
                count: CommitCount::new(2),
            })
            .unwrap(),
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
fn settings_codec_round_trips_effective_values() {
    let settings = ViewerUserSettings {
        accessibility: gtl_models::settings::ViewerAccessibility {
            ui_scale_percent: gtl_models::settings::ViewerScalePercent::try_new(200).unwrap(),
            reduce_motion: true,
        },
        language: gtl_models::settings::ViewerLanguage::PtBr,
        date_format: gtl_models::settings::ViewerDateFormat::MonthFirst,
        revision: gtl_models::settings::UserSettingsRevision::from_digest([0x22; 32]),
        focus_window_on_diff: true,
        sidebars: gtl_models::viewer::ViewerSidebarVisibility {
            files: false,
            commits: true,
        },
        projects_view: gtl_models::settings::ProjectsViewMode::Table,
        projects_sort: gtl_models::settings::ProjectsSort::BranchDescending,
        projects_page_size: gtl_models::settings::ProjectsPageSize::default(),
        configuration_path: Some("/home/dev/.config/git-tools.toml".into()),
        configured_theme: Some(ViewerTheme::Carbon),
        effective_theme: ViewerTheme::Carbon,
        render_options: gtl_wire::viewer::ViewerRenderOptions {
            wrap_lines: false,
            layout: ViewerDiffLayout::Split,
            density: ViewerDiffDensity::Full,
        },
        push_confirmation_required: true,
    };

    for focus_window_on_diff in [true, false] {
        let settings = ViewerUserSettings {
            focus_window_on_diff,
            ..settings.clone()
        };
        let mut encoded = encode_get_viewer_settings_response(settings.clone());
        let decoded = decode_get_viewer_settings_response(encoded.clone()).unwrap();
        assert_eq!(decoded, settings);
        encoded.focus_window_on_diff = None;
        assert!(decode_get_viewer_settings_response(encoded).is_err());
    }

    let mut malformed_revision = encode_get_viewer_settings_response(settings);
    malformed_revision.revision = "AA".repeat(32);
    assert!(decode_get_viewer_settings_response(malformed_revision).is_err());
}

#[test]
fn edit_settings_codec_preserves_unchanged_clear_false_and_empty_updates() {
    let request = EditSettingsRequest {
        ui_scale_percent: FieldUpdate::Update(
            gtl_models::settings::ViewerScalePercent::try_new(300).unwrap(),
        ),
        reduce_motion: FieldUpdate::Update(true),
        language: FieldUpdate::Update(gtl_models::settings::ViewerLanguage::PtBr),
        date_format: FieldUpdate::Clear,
        expected_revision: Some(gtl_models::settings::UserSettingsRevision::from_digest(
            [0x33; 32],
        )),
        focus_window_on_diff: FieldUpdate::Update(false),
        files_sidebar_visible: FieldUpdate::Update(false),
        commits_sidebar_visible: FieldUpdate::Clear,
        wrap_lines: FieldUpdate::Update(true),
        projects_view: FieldUpdate::Update(gtl_models::settings::ProjectsViewMode::Table),
        projects_sort: FieldUpdate::Update(gtl_models::settings::ProjectsSort::NameDescending),
        projects_page_size: FieldUpdate::Update(
            gtl_models::settings::ProjectsPageSize::try_new(30).unwrap(),
        ),
        theme: FieldUpdate::Clear,
        layout: FieldUpdate::Unchanged,
        density: FieldUpdate::Update(ViewerDiffDensity::Compact),
        push_confirmation_required: FieldUpdate::Update(false),
    };

    for wrap_lines in [
        FieldUpdate::Unchanged,
        FieldUpdate::Clear,
        FieldUpdate::Update(false),
        FieldUpdate::Update(true),
    ] {
        let request = EditSettingsRequest {
            reduce_motion: wrap_lines.clone(),
            focus_window_on_diff: wrap_lines.clone(),
            files_sidebar_visible: wrap_lines.clone(),
            commits_sidebar_visible: wrap_lines.clone(),
            wrap_lines,
            ..request.clone()
        };
        assert_eq!(
            decode_edit_settings_request(encode_edit_settings_request(&request)).unwrap(),
            request
        );
    }

    for date_format in gtl_models::settings::ViewerDateFormat::ALL {
        let request = EditSettingsRequest {
            date_format: FieldUpdate::Update(*date_format),
            ..request.clone()
        };
        assert_eq!(
            decode_edit_settings_request(encode_edit_settings_request(&request)).unwrap(),
            request
        );
    }

    let mut malformed_revision = encode_edit_settings_request(&request);
    malformed_revision.expected_revision = Some("not-a-revision".to_owned());
    assert_eq!(
        decode_edit_settings_request(malformed_revision),
        Err(ViewerCodecError::InvalidField {
            field: "expected_revision"
        })
    );
}

#[test]
fn project_contracts_preserve_status_and_reject_invalid_open_requests() {
    use gtl_models::paths::RepositoryRoot;
    use gtl_wire::{
        proto::viewer::projects,
        viewer::projects::{OpenViewerProject, ViewerProject, ViewerProjectPage},
    };
    let project = ViewerProject {
        comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
        id: "ALP".try_into().unwrap(),
        path: RepositoryRoot::try_new("/repos/alpha".into()).unwrap(),
        name: ProjectName::try_new("Alpha").unwrap(),
        last_rendered_at: Some("2026-09-06T10:00:00Z".try_into().unwrap()),
    };
    let response = v1::ListViewerProjectsResponse {
        projects: vec![projects::encode_project(&project)],
        total: 1,
        count_before: 0,
    };
    assert_eq!(
        projects::decode_projects(response).unwrap(),
        ViewerProjectPage::try_new(vec![project.clone()], 1, 0).unwrap()
    );
    for mode in [
        gtl_wire::viewer::projects::ViewerProjectDiffMode::Snapshot,
        gtl_wire::viewer::projects::ViewerProjectDiffMode::Live,
    ] {
        let request = OpenViewerProject {
            path: project.path.clone(),
            mode,
        };
        assert_eq!(
            projects::decode_open(projects::encode_open(&request)).unwrap(),
            request
        );
    }
    for (path, mode) in [("relative", 1), ("/repos/alpha", 0), ("/repos/alpha", 99)] {
        assert!(
            projects::decode_open(v1::OpenViewerProjectRequest {
                path: path.into(),
                mode
            })
            .is_err()
        );
    }
    assert!(
        projects::decode_projects(v1::ListViewerProjectsResponse {
            projects: vec![v1::ViewerProject {
                comparison_branch: "main".to_owned(),
                id: "invalid".into(),
                name: "Alpha".into(),
                path: "/repos/alpha".into(),
                last_rendered_at: None
            }],
            total: 1,
            count_before: 0,
        })
        .is_err()
    );
    assert!(projects::decode_open_response(v1::OpenViewerProjectResponse { tab_id: 0 }).is_err());
}

#[test]
fn project_statuses_round_trip_through_grpc_and_desktop_json() {
    use gtl_models::repository::{
        PathCount,
        status::{RepositoryStatus, StatusChanges, StatusHead, StatusUpstream},
    };
    use gtl_wire::{
        proto::viewer::projects,
        viewer::projects::{ViewerProjectBranchComparison, ViewerProjectStatus},
    };
    for status in [
        RepositoryStatus::Absent,
        RepositoryStatus::Present {
            head: StatusHead::Unavailable,
            changes: StatusChanges::Unavailable,
        },
        RepositoryStatus::Present {
            head: StatusHead::Detached,
            changes: StatusChanges::Clean,
        },
        RepositoryStatus::Present {
            head: StatusHead::Branch {
                name: "main".try_into().unwrap(),
                upstream: StatusUpstream::Missing,
            },
            changes: StatusChanges::from_counts(PathCount::new(2), PathCount::new(3)),
        },
        RepositoryStatus::Present {
            head: StatusHead::Branch {
                name: "feature".try_into().unwrap(),
                upstream: StatusUpstream::Tracking {
                    reference: "origin/main".try_into().unwrap(),
                    ahead: gtl_models::git::CommitCount::new(4),
                },
            },
            changes: StatusChanges::Clean,
        },
    ] {
        let project = ViewerProjectStatus {
            comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
            branch_comparison: ViewerProjectBranchComparison::Upstream,
            project_id: "ALP".try_into().unwrap(),
            status,
        };
        let decoded =
            projects::decode_project_status(projects::encode_project_status(project.clone()))
                .unwrap();
        assert_eq!(decoded, project);
        assert_eq!(
            serde_json::from_str::<ViewerProjectStatus>(&serde_json::to_string(&project).unwrap())
                .unwrap(),
            project
        );
    }
}

#[test]
fn project_page_bounds_survive_grpc_and_desktop_json() {
    use gtl_wire::{
        proto::viewer::projects,
        viewer::projects::{
            ListViewerProjects, ViewerProjectPage, ViewerProjectsCursor, ViewerProjectsPageSize,
        },
    };
    for cursor in [
        ViewerProjectsCursor::First,
        ViewerProjectsCursor::After("GTL".try_into().unwrap()),
        ViewerProjectsCursor::Before("GTL".try_into().unwrap()),
        ViewerProjectsCursor::Last,
    ] {
        let request = ListViewerProjects {
            sort: Some(gtl_models::settings::ProjectsSort::ChangesAscending),
            cursor,
            page_size: ViewerProjectsPageSize::try_new(100).unwrap(),
        };
        assert_eq!(
            projects::decode_list(projects::encode_list(request.clone())).unwrap(),
            request
        );
        assert_eq!(
            serde_json::from_str::<ListViewerProjects>(&serde_json::to_string(&request).unwrap())
                .unwrap(),
            request
        );
    }
    for size in [0, 101, u32::MAX] {
        assert!(ViewerProjectsPageSize::try_new(size).is_err());
        assert!(serde_json::from_value::<ViewerProjectsPageSize>(serde_json::json!(size)).is_err());
    }
    assert!(
        serde_json::from_value::<ListViewerProjects>(
            serde_json::json!({"cursor": {"After": "invalid"}, "page_size": 15})
        )
        .is_err()
    );
    assert!(ViewerProjectPage::try_new(vec![], 0, 1).is_none());
    assert!(
        serde_json::from_value::<ViewerProjectPage>(
            serde_json::json!({"projects": [], "total": 0, "count_before": 1})
        )
        .is_err()
    );
    assert!(
        projects::decode_project_status(v1::GetViewerProjectStatusResponse::default()).is_err()
    );
}

#[test]
fn file_filter_requests_round_trip_modes_and_normalize_the_client_boundary() {
    use gtl_wire::{proto::viewer::file_filters, viewer::file_filters::SetViewerFileFilters};
    for mode in [ExtensionFilterMode::Hide, ExtensionFilterMode::Only] {
        let request = SetViewerFileFilters {
            tab_id: gtl_models::viewer::ViewerTabId::try_new(7).unwrap(),
            filter: ExtensionFilter::new(mode, FileExtensions::new(["lock"])),
        };
        assert_eq!(
            file_filters::decode_set(file_filters::encode_set(request.clone())).unwrap(),
            request
        );
    }
    let decoded = file_filters::decode_set(v1::SetViewerFileFiltersRequest {
        tab_id: 7,
        filter: Some(v1::ViewerExtensionFilter {
            mode: v1::ViewerExtensionFilterMode::Only as i32,
            extensions: vec![" .MD ".into(), "lock".into(), "md".into()],
        }),
    })
    .unwrap();
    assert_eq!(
        decoded.filter,
        ExtensionFilter::new(
            ExtensionFilterMode::Only,
            FileExtensions::new(["lock", "md"])
        )
    );
    for invalid in [
        None,
        Some(v1::ViewerExtensionFilter {
            mode: v1::ViewerExtensionFilterMode::Unspecified as i32,
            extensions: Vec::new(),
        }),
        Some(v1::ViewerExtensionFilter {
            mode: v1::ViewerExtensionFilterMode::Hide as i32,
            extensions: vec!["a/b".into()],
        }),
    ] {
        assert!(
            file_filters::decode_set(v1::SetViewerFileFiltersRequest {
                tab_id: 7,
                filter: invalid,
            })
            .is_err()
        );
    }
}

#[test]
fn accessibility_patch_preserves_clear_and_rejects_invalid_scale() {
    for update in [
        FieldUpdate::Unchanged,
        FieldUpdate::Clear,
        FieldUpdate::Update(gtl_models::settings::ViewerScalePercent::try_new(125).unwrap()),
    ] {
        let request = EditSettingsRequest {
            ui_scale_percent: update,
            ..Default::default()
        };
        assert_eq!(
            decode_edit_settings_request(encode_edit_settings_request(&request)).unwrap(),
            request
        );
    }
    for value in [0, 99, 126, 301, u32::MAX] {
        let request = v1::EditSettingsRequest {
            ui_scale_percent: Some(v1::ViewerScaleFieldUpdate {
                operation: Some(v1::viewer_scale_field_update::Operation::Update(value)),
            }),
            ..Default::default()
        };
        assert_eq!(
            decode_edit_settings_request(request),
            Err(ViewerCodecError::InvalidField {
                field: "ui_scale_percent"
            })
        );
    }
}
