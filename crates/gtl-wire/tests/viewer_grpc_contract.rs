use gtl_models::{
    diffs::ExcludedExtensions,
    paths::ProjectName,
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
        ViewerActiveState, ViewerCodeSpan, ViewerDiffDensity, ViewerDiffExclusions,
        ViewerDiffFileId, ViewerDiffLayout, ViewerDiffSearchDirection, ViewerDiffSearchMatch,
        ViewerDiffSearchResult, ViewerFeedback, ViewerFileSearchResult, ViewerHistoryEntry,
        ViewerHistoryPage, ViewerPreferences, ViewerProjectDiffExclusions,
        ViewerProjectSettingsUpdate, ViewerRecipeKind, ViewerRowEvent, ViewerShell,
        ViewerSyntaxClass, ViewerTab, ViewerTabKind, ViewerTabState, ViewerTheme,
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
        focus_request_version: Some(ViewerVersion::new(3)),
        tabs: vec![ViewerTab {
            pinned: false,
            id: ViewerTabId::try_new(7).unwrap(),
            label: "git-tools".into(),
            kind: ViewerTabKind::Live,
            state: ViewerTabState::Ready,
        }],
        active: ViewerActiveState::Empty,
        preferences: ViewerPreferences {
            sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
            theme: ViewerTheme::Graphite,
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
fn ready_shell_metadata_survives_protobuf_and_rejects_invalid_content_ids()
-> Result<(), Box<dyn std::error::Error>> {
    use prost::Message as _;
    let active = v1::ViewerActiveView {
        modified_files: false,
        row_source: v1::ViewerRowSourceState::Ready as i32,
        identity: Some(encode_viewer_view_identity(viewer_identity().unwrap())),
        content_id: Some(vec![42; 32]),
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
    };
    let shell = ViewerShell {
        version: ViewerVersion::new(1),
        focus_request_version: None,
        tabs: Vec::new(),
        active: ViewerActiveState::Empty,
        preferences: ViewerPreferences {
            sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
            theme: ViewerTheme::Dark,
            render_options: viewer_identity().unwrap().render_options,
            keybindings: ViewerKeybindings::default(),
        },
        feedback: None,
    };
    let mut encoded = encode_viewer_shell(shell).unwrap();
    encoded.active = Some(v1::ViewerActiveState {
        state: Some(v1::viewer_active_state::State::Ready(Box::new(
            v1::ViewerReadyState {
                view: Some(active.clone()),
            },
        ))),
    });
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
            long_line_character_count: None,
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
        sidebars: gtl_models::viewer::ViewerSidebarVisibility {
            files: false,
            commits: true,
        },
        projects_view: gtl_models::settings::ProjectsViewMode::Table,
        configuration_path: Some("/home/dev/.config/git-tools.toml".into()),
        configured_theme: Some(ViewerTheme::Hearth),
        effective_theme: ViewerTheme::Hearth,
        render_options: gtl_wire::viewer::ViewerRenderOptions {
            wrap_lines: false,
            layout: ViewerDiffLayout::Split,
            density: ViewerDiffDensity::Full,
        },
        push_confirmation_required: true,
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: ExcludedExtensions::new(["lock"]),
            projects: vec![ViewerProjectDiffExclusions {
                project_name: ProjectName::try_new("git-tools").unwrap(),
                extensions: ExcludedExtensions::new(["snap"]),
                excluded_from_push_all: true,
            }],
        },
    };

    let encoded = encode_get_viewer_settings_response(settings.clone());
    let decoded = decode_get_viewer_settings_response(encoded).unwrap();

    assert_eq!(decoded, settings);
}

#[test]
fn edit_settings_codec_preserves_unchanged_clear_false_and_empty_updates() {
    let request = EditSettingsRequest {
        files_sidebar_visible: FieldUpdate::Update(false),
        commits_sidebar_visible: FieldUpdate::Clear,
        wrap_lines: FieldUpdate::Update(true),
        projects_view: FieldUpdate::Update(gtl_models::settings::ProjectsViewMode::Table),
        theme: FieldUpdate::Clear,
        layout: FieldUpdate::Unchanged,
        density: FieldUpdate::Update(ViewerDiffDensity::Compact),
        push_confirmation_required: FieldUpdate::Update(false),
        default_diff_exclusions: FieldUpdate::Update(ExcludedExtensions::default()),
        projects: FieldUpdate::Update(vec![ViewerProjectSettingsUpdate {
            project_name: ProjectName::try_new("git-tools").unwrap(),
            excluded_from_push_all: true,
            diff_exclusions: ExcludedExtensions::new(["lock"]),
        }]),
    };

    for wrap_lines in [
        FieldUpdate::Unchanged,
        FieldUpdate::Clear,
        FieldUpdate::Update(false),
        FieldUpdate::Update(true),
    ] {
        let request = EditSettingsRequest {
            files_sidebar_visible: wrap_lines.clone(),
            commits_sidebar_visible: wrap_lines.clone(),
            wrap_lines,
            ..request.clone()
        };
        assert_eq!(
            decode_edit_settings_request(encode_edit_settings_request(request.clone())).unwrap(),
            request
        );
    }
}

#[test]
fn project_contracts_preserve_status_and_reject_invalid_open_requests() {
    use gtl_models::{paths::RepositoryRoot, repository::status::RepositoryStatus};
    use gtl_wire::{
        proto::viewer::projects,
        viewer::projects::{OpenViewerProject, ViewerProject, ViewerProjectBranchComparison},
    };
    let project = ViewerProject {
        comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
        branch_comparison: ViewerProjectBranchComparison::Upstream,
        path: RepositoryRoot::try_new("/repos/alpha".into()).unwrap(),
        name: ProjectName::try_new("Alpha").unwrap(),
        status: RepositoryStatus::Absent,
        last_rendered_at: Some("2026-09-06T10:00:00Z".try_into().unwrap()),
    };
    let response = v1::ListViewerProjectsResponse {
        projects: vec![projects::encode_project(project.clone())],
    };
    assert_eq!(
        projects::decode_projects(response).unwrap(),
        vec![project.clone()]
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
                branch_comparison: None,
                path: "/repos/alpha".into(),
                status: None,
                last_rendered_at: None
            }]
        })
        .is_err()
    );
    assert!(projects::decode_open_response(v1::OpenViewerProjectResponse { tab_id: 0 }).is_err());
}

#[test]
fn project_statuses_round_trip_through_grpc_and_desktop_json() {
    use gtl_models::{
        paths::RepositoryRoot,
        repository::{
            PathCount,
            status::{RepositoryStatus, StatusChanges, StatusHead, StatusUpstream},
        },
    };
    use gtl_wire::{
        proto::viewer::projects,
        viewer::projects::{ViewerProject, ViewerProjectBranchComparison},
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
        let project = ViewerProject {
            comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
            branch_comparison: ViewerProjectBranchComparison::Upstream,
            path: RepositoryRoot::try_new("/repos/alpha".into()).unwrap(),
            name: ProjectName::try_new("Alpha").unwrap(),
            status,
            last_rendered_at: None,
        };
        let decoded = projects::decode_projects(v1::ListViewerProjectsResponse {
            projects: vec![projects::encode_project(project.clone())],
        })
        .unwrap();
        assert_eq!(decoded, vec![project.clone()]);
        assert_eq!(
            serde_json::from_str::<ViewerProject>(&serde_json::to_string(&project).unwrap())
                .unwrap(),
            project
        );
    }
}
