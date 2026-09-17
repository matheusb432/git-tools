use gtl_models::{
    diffs::{CommitId, ExcludedExtensions},
    git::{BranchName, GitHead, GitRevision},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRelativePath},
    timestamps::MachineTimestamp,
    viewer::{
        HistoryPage, HistoryPageCount, HistoryPageNumber, HistoryPagePosition, HistoryRenderCount,
        RenderHistoryId, ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId,
        ViewerVersion,
    },
};
use gtl_wire::viewer::*;
use serde_json::json;

const COMMIT_ID: &str = "abcdef0123456789abcdef0123456789abcdef01";

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn commit_id() -> TestResult<CommitId> {
    Ok(COMMIT_ID.try_into()?)
}

fn tab_id(value: u64) -> TestResult<ViewerTabId> {
    Ok(ViewerTabId::try_new(value)?)
}

fn render_id(value: i64) -> TestResult<RenderHistoryId> {
    Ok(RenderHistoryId::try_new(value)?)
}

fn page_number(value: u32) -> TestResult<HistoryPageNumber> {
    Ok(HistoryPageNumber::try_new(value)?)
}

fn page_position(number: u32, count: u32) -> TestResult<HistoryPagePosition> {
    Ok(HistoryPagePosition::Page(HistoryPage::new(
        page_number(number)?,
        HistoryPageCount::try_new(count)?,
    )?))
}

fn revision(value: &str) -> TestResult<GitRevision> {
    Ok(GitRevision::try_new(value.to_owned())?)
}

fn head(value: &str) -> TestResult<GitHead> {
    if value == "HEAD" {
        return Ok(GitHead::Detached);
    }
    Ok(GitHead::Branch(BranchName::try_new(value.to_owned())?))
}

fn project_name(value: &str) -> TestResult<ProjectName> {
    Ok(ProjectName::try_new(value.to_owned())?)
}

fn relative_path(value: &str) -> TestResult<RepositoryRelativePath> {
    Ok(RepositoryRelativePath::try_new(value.into())?)
}

fn absolute_file_path(value: &str) -> TestResult<AbsoluteFilePath> {
    Ok(AbsoluteFilePath::try_new(value.into())?)
}

fn identity() -> TestResult<ViewerViewIdentity> {
    Ok(ViewerViewIdentity {
        tab_id: tab_id(7)?,
        range_generation: ViewerRangeGeneration::new(11),
        selection_generation: ViewerSelectionGeneration::new(13),
        render_options: ViewerRenderOptions {
            wrap_lines: false,
            layout: ViewerDiffLayout::Split,
            density: ViewerDiffDensity::Full,
        },
    })
}

#[test]
fn closed_value_tokens_keep_their_wire_contracts() {
    let values = [
        serde_json::to_value(ViewerTheme::Graphite).unwrap(),
        serde_json::to_value(ViewerDiffLayout::Unified).unwrap(),
        serde_json::to_value(ViewerDiffDensity::Compact).unwrap(),
        serde_json::to_value(ViewerTabKind::Snapshot).unwrap(),
        serde_json::to_value(ViewerFileStatus::Renamed).unwrap(),
        serde_json::to_value(ViewerRecipeKind::MergeDiff).unwrap(),
    ];

    assert_eq!(
        values,
        [
            json!("graphite"),
            json!("unified"),
            json!("compact"),
            json!("snapshot"),
            json!("renamed"),
            json!("merge_diff"),
        ]
    );
}

#[test]
fn viewer_failure_codes_use_full_variant_names() {
    for (code, name) in [
        (
            ViewerFailureCode::RepositoryDirectoryNotFound,
            "RepositoryDirectoryNotFound",
        ),
        (
            ViewerFailureCode::RepositoryDirectoryNotGitRepository,
            "RepositoryDirectoryNotGitRepository",
        ),
        (ViewerFailureCode::SourceUnavailable, "SourceUnavailable"),
        (ViewerFailureCode::RenderFailed, "RenderFailed"),
    ] {
        let value = serde_json::to_value(code).unwrap();

        assert_eq!(code.as_str(), name);
        assert_eq!(value, json!(name));
        assert_eq!(
            serde_json::from_value::<ViewerFailureCode>(value).unwrap(),
            code
        );
    }

    for abbreviated in ["DirNotFound", "DirNotGitRepo"] {
        assert!(serde_json::from_value::<ViewerFailureCode>(json!(abbreviated)).is_err());
    }
}

#[test]
fn tagged_enums_pin_each_wire_discriminator() -> TestResult {
    assert_eq!(
        serde_json::to_value(ViewerTabState::Broken).unwrap(),
        json!({"state": "broken"})
    );
    assert_eq!(
        serde_json::to_value(ViewerCommitSelection::Error {
            id: commit_id()?,
            message: "The commit could not be rendered.".into(),
        })
        .unwrap(),
        json!({
            "state": "error",
            "id": COMMIT_ID,
            "message": "The commit could not be rendered."
        })
    );
    assert_eq!(
        serde_json::to_value(ViewerActiveState::Pending { tab_id: tab_id(7)? }).unwrap(),
        json!({"state": "pending", "tab_id": 7})
    );
    assert_eq!(
        serde_json::to_value(ViewerHistoryCursor::OlderThan {
            render_id: render_id(41)?,
            page: page_number(3)?,
        })
        .unwrap(),
        json!({"cursor": "older_than", "render_id": 41, "page": 3})
    );
    assert_eq!(
        serde_json::to_value(SetViewerPreference::Theme(ViewerTheme::Glacier)).unwrap(),
        json!({"preference": "theme", "value": "glacier"})
    );
    assert_eq!(
        serde_json::to_value(ViewerFeedback::SnapshotRecipesSkipped {
            labels: vec!["api".into()],
        })
        .unwrap(),
        json!({"kind": "snapshot_recipes_skipped", "labels": ["api"]})
    );
    Ok(())
}

#[test]
fn history_cursor_rejects_zero_during_wire_deserialization() {
    let result = serde_json::from_value::<ViewerHistoryCursor>(
        json!({"cursor": "older_than", "render_id": 41, "page": 0}),
    );

    assert!(result.is_err());
}

#[test]
fn ready_shell_contains_semantic_metadata_without_diff_rows() -> TestResult {
    let shell = semantic_shell()?;

    let value = serde_json::to_value(&shell).unwrap();

    assert_eq!(value["focus_request_version"], 20);
    assert_eq!(value["active"]["state"], "ready");
    assert_eq!(
        value["active"]["view"]["files"][0]["anchor_id"],
        "file-src-lib-rs"
    );
    assert_eq!(value["active"]["view"]["files"][0]["id"], "file-0");
    assert_eq!(value["active"]["view"]["commits"][0]["id"], COMMIT_ID);
    assert_eq!(
        value["active"]["view"]["commits"][0]["date"],
        "2026-08-09 10:00"
    );
    assert_eq!(
        value["active"]["view"]["commits"][0]["iso"],
        "2026-08-09T10:00:00Z"
    );
    assert!(value.pointer("/active/view/commits/0/sha").is_none());
    assert!(
        value
            .pointer("/active/view/commits/0/abbreviated_sha")
            .is_none()
    );
    assert!(value.pointer("/active/view/files/0/lines").is_none());
    assert!(value.pointer("/active/view/repository_root").is_none());
    assert_eq!(
        value["preferences"]["keybindings"]["search_files"],
        "ctrl+p"
    );
    assert_eq!(
        value["preferences"]["keybindings"]["search_text_in_all_files"],
        "ctrl+f"
    );
    assert_eq!(serde_json::from_value::<ViewerShell>(value).unwrap(), shell);

    Ok(())
}

fn semantic_shell() -> TestResult<ViewerShell> {
    let identity = identity()?;
    Ok(ViewerShell {
        version: ViewerVersion::new(23),
        focus_request_version: Some(ViewerVersion::new(20)),
        tabs: vec![ViewerTab {
            pinned: false,
            id: tab_id(7)?,
            label: "git-tools".into(),
            kind: ViewerTabKind::Live,
            state: ViewerTabState::Ready,
        }],
        active: ViewerActiveState::Ready {
            view: Box::new(ViewerActiveView {
                modified_files: false,
                row_source: gtl_wire::viewer::ViewerRowSourceState::Ready,
                identity,
                content_id: ViewerRowContentId::from_digest([42; 32]),
                title: "feature vs main".into(),
                repository_name: project_name("git-tools")?,
                branch: head("feature")?,
                upstream: revision("origin/main")?,
                command: ViewerCommandLine {
                    lead: "git diff ".into(),
                    range: "main...HEAD".into(),
                    trail: String::new(),
                },
                files: vec![ViewerFileSummary {
                    source_id: None,
                    id: ViewerDiffFileId::for_index(0),
                    path: relative_path("src/lib.rs")?,
                    absolute_path: absolute_file_path("/repos/git-tools/src/lib.rs")?,
                    anchor_id: "file-src-lib-rs".into(),
                    added: gtl_models::diffs::DiffLineCount::new(4),
                    removed: gtl_models::diffs::DiffLineCount::new(2),
                    status: ViewerFileStatus::Modified,
                    can_open_in_editor: true,
                    initially_expanded: true,
                    row_count: 1,
                }],
                commits_label: "1 commit".into(),
                commit_count: 1,
                commits: vec![ViewerCommitSummary {
                    id: commit_id()?,
                    subject: "feat: add shell".into(),
                    body: String::new(),
                    committed_at: MachineTimestamp::try_from("2026-08-09T10:00:00Z")?,
                    is_merge: false,
                }],
                commit_selection: ViewerCommitSelection::None,
                footer: ViewerFooter {
                    command: "gtl diff".into(),
                },
                exclusions: Some(ViewerAppliedExclusions {
                    extensions: ExcludedExtensions::new(["md"]),
                    hidden_paths: vec![relative_path("README.md")?],
                }),
            }),
        },
        preferences: ViewerPreferences {
            sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
            theme: ViewerTheme::Dark,
            render_options: identity.render_options,
            keybindings: gtl_models::viewer::ViewerKeybindings::default(),
        },
        feedback: None,
    })
}

#[test]
fn commit_selection_requests_reject_invalid_commit_ids() {
    assert!(
        serde_json::from_value::<SelectViewerCommit>(json!({
            "tab_id": 7,
            "id": "abcdef"
        }))
        .is_err()
    );
}

#[test]
fn commit_summary_rejects_a_display_value_inconsistent_with_its_machine_timestamp() {
    let summary = serde_json::from_value::<ViewerCommitSummary>(json!({
        "id": COMMIT_ID,
        "subject": "subject",
        "body": "",
        "date": "2026-08-09 11:00",
        "iso": "2026-08-09T10:00:00Z",
        "is_merge": false
    }));

    assert!(summary.is_err());
}

#[test]
fn commit_selection_compares_complete_typed_identities() -> TestResult {
    let selected_id: CommitId = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".try_into()?;
    let same_prefix_different_id: CommitId =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab".try_into()?;
    let tab_id = tab_id(7)?;
    let selection = ViewerCommitSelection::Ready {
        id: selected_id.clone(),
    };

    assert!(matches!(
        make_commit_selection_action(&selection, tab_id, selected_id),
        CommitSelectionAction::UnselectCommit
    ));
    assert!(matches!(
        make_commit_selection_action(&selection, tab_id, same_prefix_different_id.clone()),
        CommitSelectionAction::FetchCommit(SelectViewerCommit {
            tab_id: fetched_tab_id,
            id,
        }) if fetched_tab_id == tab_id && id == same_prefix_different_id
    ));
    Ok(())
}

#[test]
fn diff_history_and_settings_shapes_round_trip() -> TestResult {
    let history = ViewerHistoryPage {
        projects: Vec::new(),
        entries: vec![ViewerHistoryEntry {
            id: render_id(31)?,
            title: "Release diff".into(),
            repository_name: project_name("git-tools")?,
            kind: ViewerRecipeKind::MergeDiff,
            range_label: "main...release".into(),
            rendered_at: MachineTimestamp::try_from("2026-08-09T10:00:00Z")?,
        }],
        total_count: HistoryRenderCount::new(1),
        position: page_position(1, 1)?,
        has_newer: false,
        has_older: false,
    };
    let settings = ViewerUserSettings {
        revision: gtl_models::settings::UserSettingsRevision::from_digest([0x11; 32]),
        focus_window_on_diff: true,
        sidebars: gtl_models::viewer::ViewerSidebarVisibility {
            files: false,
            commits: true,
        },
        projects_view: gtl_models::settings::ProjectsViewMode::Table,
        projects_sort: gtl_models::settings::ProjectsSort::Branch,
        projects_page_size: gtl_models::settings::ProjectsPageSize::default(),
        configuration_path: Some("/home/user/.config/git-tools/config.toml".into()),
        configured_theme: None,
        effective_theme: ViewerTheme::Dark,
        render_options: ViewerRenderOptions {
            wrap_lines: false,
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        },
        push_confirmation_required: true,
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: ExcludedExtensions::new(["md"]),
            projects: vec![ViewerProjectDiffExclusions {
                configured: true,
                project_name: project_name("git-tools")?,
                extensions: ExcludedExtensions::new(["js"]),
                excluded_from_push_all: false,
            }],
        },
    };

    let history_json = serde_json::to_value(&history).unwrap();
    assert_eq!(
        history_json["entries"][0]["rendered_at"],
        "2026-08-09T10:00:00Z"
    );
    assert!(history_json.pointer("/entries/0/recipe").is_none());
    assert!(!history_json.to_string().contains("/repos/git-tools"));
    assert_eq!(
        serde_json::from_value::<ViewerHistoryPage>(history_json).unwrap(),
        history
    );
    assert_eq!(
        serde_json::from_value::<ViewerUserSettings>(serde_json::to_value(&settings).unwrap())
            .unwrap(),
        settings
    );
    Ok(())
}

#[test]
fn history_copy_payload_carries_server_formatted_json() -> TestResult {
    let json = r#"{
  "id": 31,
  "title": "Release diff",
  "repo_name": "git-tools",
  "kind": "merge-diff"
}"#;
    let payload = ViewerHistoryCopyPayload {
        json: json.to_owned(),
    };

    assert_eq!(
        serde_json::to_value(payload).unwrap(),
        json!({"json": json})
    );
    assert_eq!(
        serde_json::to_value(GetViewerHistoryCopy {
            render_id: render_id(31)?,
        })
        .unwrap(),
        json!({"render_id": 31})
    );
    Ok(())
}

#[test]
fn row_content_id_has_a_fixed_width_serde_contract() {
    let id = ViewerRowContentId::from_digest([42; 32]);
    let encoded = serde_json::to_value(id).unwrap();
    assert_eq!(encoded, json!(vec![42; 32]));
    assert_eq!(
        serde_json::from_value::<ViewerRowContentId>(encoded).unwrap(),
        id
    );
    for invalid in [
        json!([]),
        json!(vec![42; 31]),
        json!(vec![42; 33]),
        json!(vec![256; 32]),
        json!(null),
    ] {
        assert!(serde_json::from_value::<ViewerRowContentId>(invalid).is_err());
    }
}

#[test]
fn row_windows_validate_bounds_when_constructed_and_deserialized() {
    use gtl_wire::viewer::ViewerRowRange;

    let range = ViewerRowRange::try_new(512, 64).unwrap();
    assert_eq!(range.start(), 512);
    assert_eq!(range.end(), 576);
    let encoded = serde_json::to_string(&range).unwrap();
    assert_eq!(
        serde_json::from_str::<ViewerRowRange>(&encoded).unwrap(),
        range
    );
    for (start, count) in [(0, 0), (0, 513), (u32::MAX, 1)] {
        assert!(ViewerRowRange::try_new(start, count).is_err());
        assert!(
            serde_json::from_value::<ViewerRowRange>(
                serde_json::json!({ "start": start, "count": count })
            )
            .is_err()
        );
    }
}
