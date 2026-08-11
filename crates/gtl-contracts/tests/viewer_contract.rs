use std::path::PathBuf;

use gtl_contracts::{
    recipes::{Recipe, RecipeOp, RecipeSource},
    viewer::*,
};
use serde_json::json;

fn identity() -> ViewerViewIdentity {
    ViewerViewIdentity {
        tab_id: 7,
        range_generation: 11,
        selection_generation: 13,
        render_options: ViewerRenderOptions {
            layout: ViewerDiffLayout::Split,
            density: ViewerDiffDensity::Full,
        },
    }
}

fn recipe() -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(PathBuf::from("/repos/git-tools")),
        op: RecipeOp::MergeDiff {
            base: Some("main".into()),
            pinned: None,
        },
        name: Some("release".into()),
    }
}

#[test]
fn closed_value_tokens_are_snake_case() {
    let values = [
        serde_json::to_value(ViewerTheme::Graphite).expect("theme serializes"),
        serde_json::to_value(ViewerDiffLayout::Unified).expect("layout serializes"),
        serde_json::to_value(ViewerDiffDensity::Compact).expect("density serializes"),
        serde_json::to_value(ViewerTabKind::Snapshot).expect("tab kind serializes"),
        serde_json::to_value(ViewerFileStatus::Renamed).expect("file status serializes"),
        serde_json::to_value(ViewerRecipeKind::MergeDiff).expect("recipe kind serializes"),
        serde_json::to_value(ViewerHistoryCopyKind::MergeDiff)
            .expect("history copy kind serializes"),
        serde_json::to_value(ViewerFailureCode::RepositoryDirectoryNotFound)
            .expect("failure code serializes"),
        serde_json::to_value(ViewerResource::DiffLines).expect("resource serializes"),
        serde_json::to_value(ViewerResource::HistoryEntry).expect("resource serializes"),
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
            json!("merge-diff"),
            json!("DirNotFound"),
            json!("diff_lines"),
            json!("history_entry"),
        ]
    );
}

#[test]
fn tagged_enums_pin_each_wire_discriminator() {
    assert_eq!(
        serde_json::to_value(ViewerTabState::Broken).expect("tab state serializes"),
        json!({"state": "broken"})
    );
    assert_eq!(
        serde_json::to_value(ViewerCommitSelection::Error {
            sha: "abcdef".into(),
            message: "The commit could not be rendered.".into(),
        })
        .expect("commit selection serializes"),
        json!({
            "state": "error",
            "sha": "abcdef",
            "message": "The commit could not be rendered."
        })
    );
    assert_eq!(
        serde_json::to_value(ViewerActiveState::Pending { tab_id: 7 })
            .expect("active state serializes"),
        json!({"state": "pending", "tab_id": 7})
    );
    assert_eq!(
        serde_json::to_value(ViewerHistoryCursor::OlderThan {
            render_id: 41,
            page: 3,
        })
        .expect("history cursor serializes"),
        json!({"cursor": "older_than", "render_id": 41, "page": 3})
    );
    assert_eq!(
        serde_json::to_value(SetViewerPreference::Theme(ViewerTheme::Glacier))
            .expect("preference serializes"),
        json!({"preference": "theme", "value": "glacier"})
    );
    assert_eq!(
        serde_json::to_value(ViewerFeedback::SnapshotRecipesSkipped {
            labels: vec!["api".into()],
        })
        .expect("feedback serializes"),
        json!({"kind": "snapshot_recipes_skipped", "labels": ["api"]})
    );
    assert_eq!(
        serde_json::to_value(ViewerApiError::NotFound {
            resource: ViewerResource::DiffFile,
        })
        .expect("API error serializes"),
        json!({"kind": "not_found", "resource": "diff_file"})
    );
}

#[test]
fn ready_shell_contains_semantic_metadata_without_diff_rows() {
    let shell = ViewerShell {
        revision: 23,
        tabs: vec![ViewerTab {
            id: 7,
            label: "git-tools".into(),
            kind: ViewerTabKind::Live,
            state: ViewerTabState::Ready,
        }],
        active: ViewerActiveState::Ready {
            view: Box::new(ViewerActiveView {
                identity: identity(),
                title: "feature vs main".into(),
                repository_name: "git-tools".into(),
                branch: "feature".into(),
                upstream: "origin/main".into(),
                command: ViewerCommandLine {
                    lead: "git diff ".into(),
                    range: "main...HEAD".into(),
                    trail: String::new(),
                },
                files: vec![ViewerFileSummary {
                    id: ViewerDiffFileId::for_index(0),
                    path: "src/lib.rs".into(),
                    absolute_path: "/repos/git-tools/src/lib.rs".into(),
                    anchor_id: "file-src-lib-rs".into(),
                    added: 4,
                    removed: 2,
                    status: ViewerFileStatus::Modified,
                    can_open_in_editor: true,
                    initially_expanded: true,
                }],
                commits_label: "1 commit".into(),
                commits: vec![ViewerCommitSummary {
                    sha: "abcdef0123456789".into(),
                    abbreviated_sha: "abcdef0123".into(),
                    subject: "feat: add shell".into(),
                    body: String::new(),
                    date: "2 hours ago".into(),
                    iso: "2026-08-09T10:00:00Z".into(),
                    is_merge: false,
                }],
                commit_selection: ViewerCommitSelection::None,
                footer: ViewerFooter {
                    command: "gtl diff".into(),
                    note: "generated locally".into(),
                },
                exclusions: Some(ViewerAppliedExclusions {
                    extensions: vec!["md".into()],
                    hidden_paths: vec!["README.md".into()],
                }),
            }),
        },
        preferences: ViewerPreferences {
            theme: ViewerTheme::Dark,
            render_options: identity().render_options,
        },
        feedback: None,
    };

    let value = serde_json::to_value(&shell).expect("shell serializes");
    assert_eq!(value["active"]["state"], "ready");
    assert_eq!(
        value["active"]["view"]["files"][0]["anchor_id"],
        "file-src-lib-rs"
    );
    assert_eq!(value["active"]["view"]["files"][0]["id"], "file-0");
    assert!(value.pointer("/active/view/files/0/lines").is_none());
    assert!(value.pointer("/active/view/repository_root").is_none());
    assert_eq!(
        serde_json::from_value::<ViewerShell>(value).expect("shell deserializes"),
        shell
    );
}

#[test]
fn raw_diff_line_pages_pin_opaque_addressing_and_identity_echoes() {
    let file = ViewerDiffFileId::for_index(3);
    let cursor = ViewerDiffCursor::new(8);
    let request = LoadViewerDiffLines {
        identity: identity(),
        file: file.clone(),
        cursor,
    };
    let page = ViewerDiffLines {
        identity: identity(),
        file,
        cursor,
        lines: vec!["@@ -1 +1 @@".into(), "+client rendered".into()],
        next: Some(ViewerDiffCursor::new(10)),
    };

    assert_eq!(
        serde_json::to_value(&request).expect("line request serializes"),
        json!({
            "identity": {
                "tab_id": 7,
                "range_generation": 11,
                "selection_generation": 13,
                "render_options": {"layout": "split", "density": "full"}
            },
            "file": "file-3",
            "cursor": 8
        })
    );
    assert_eq!(
        serde_json::from_value::<LoadViewerDiffLines>(
            serde_json::to_value(&request).expect("line request serializes")
        )
        .expect("line request deserializes"),
        request
    );
    let value = serde_json::to_value(&page).expect("line page serializes");
    assert_eq!(value["file"], "file-3");
    assert_eq!(value["cursor"], 8);
    assert_eq!(value["next"], 10);
    assert_eq!(
        serde_json::from_value::<ViewerDiffLines>(value).expect("line page deserializes"),
        page
    );
}

#[test]
fn diff_history_and_settings_shapes_round_trip() {
    let history = ViewerHistoryPage {
        entries: vec![ViewerHistoryEntry {
            id: 31,
            title: "Release diff".into(),
            repository_name: "git-tools".into(),
            kind: ViewerRecipeKind::MergeDiff,
            range_label: "main...release".into(),
            rendered_at: "2026-08-09T10:00:00Z".into(),
        }],
        total_count: 1,
        page_number: 1,
        page_count: 1,
        has_newer: false,
        has_older: false,
    };
    let settings = ViewerUserSettings {
        configuration_path: Some("/home/user/.config/git-tools/config.toml".into()),
        configured_theme: None,
        effective_theme: ViewerTheme::Dark,
        render_options: ViewerRenderOptions {
            layout: ViewerDiffLayout::Unified,
            density: ViewerDiffDensity::Compact,
        },
        push_confirmation_required: true,
        diff_exclusions: ViewerDiffExclusions {
            default_extensions: vec!["md".into()],
            projects: vec![ViewerProjectDiffExclusions {
                project_name: "git-tools".into(),
                extensions: vec!["js".into()],
            }],
        },
    };

    let history_json = serde_json::to_value(&history).expect("history serializes");
    assert!(history_json.pointer("/entries/0/recipe").is_none());
    assert!(!history_json.to_string().contains("/repos/git-tools"));
    assert_eq!(
        serde_json::from_value::<ViewerHistoryPage>(history_json).expect("history deserializes"),
        history
    );
    assert_eq!(
        serde_json::from_value::<ViewerUserSettings>(
            serde_json::to_value(&settings).expect("settings serialize")
        )
        .expect("settings deserialize"),
        settings
    );
}

#[test]
fn explicit_history_copy_payload_keeps_the_established_recipe_json_shape() {
    let payload = ViewerHistoryCopyPayload {
        id: 31,
        title: "Release diff".into(),
        repo_name: "git-tools".into(),
        kind: ViewerHistoryCopyKind::MergeDiff,
        range_label: "main...release".into(),
        rendered_at: "2026-08-09T10:00:00Z".into(),
        recipe: recipe(),
    };

    assert_eq!(
        serde_json::to_value(payload).expect("history copy payload serializes"),
        json!({
            "id": 31,
            "title": "Release diff",
            "repo_name": "git-tools",
            "kind": "merge-diff",
            "range_label": "main...release",
            "rendered_at": "2026-08-09T10:00:00Z",
            "recipe": {
                "source": {"kind": "local_repo", "value": "/repos/git-tools"},
                "op": {"op": "merge_diff", "base": "main"},
                "name": "release"
            }
        })
    );
    assert_eq!(
        serde_json::to_value(GetViewerHistoryCopy { render_id: 31 })
            .expect("history copy request serializes"),
        json!({"render_id": 31})
    );
}
