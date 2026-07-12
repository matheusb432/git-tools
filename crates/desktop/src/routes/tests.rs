use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::Arc,
};

use application::{
    ports::{LiveViewRecord, RecentRenderRecord, RepoProbeResult},
    testing::{FakeDiffSource, FakeRepoProbe, InMemoryAppStateStore},
};
use domain::{
    diffs::Commit,
    viewer::{DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, ViewerTabId, ViewerTabKind},
};
use gtl_recipe::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget};
use tauri::http::{Method, Request, StatusCode};

use super::{
    ErrorTarget, HTML_CONTENT_TYPE, RouteError, TEXT_CONTENT_TYPE, ViewerApp, error_response,
    history::to_viewer_entry, process_pending, serve_app,
};
use crate::{protocol_config, test_support};

const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1 +1 @@\n\
-old\n\
+new\n";

fn source() -> FakeDiffSource {
    FakeDiffSource {
        top_level: Some("/repo".into()),
        branch: "feature".into(),
        upstream: Some("origin/main".into()),
        commits: vec![Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }],
        diff_output: SINGLE_FILE_DIFF.into(),
        ..Default::default()
    }
}

fn recipe() -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(PathBuf::from("/repo")),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: None,
    }
}

fn request(path: &str) -> Request<Vec<u8>> {
    Request::builder()
        .method(Method::GET)
        .uri(format!(
            "{}{}",
            protocol_config::APP_URL,
            path.trim_start_matches('/')
        ))
        .body(Vec::new())
        .expect("request builds")
}

#[test]
fn recent_render_mapping_preserves_task_five_viewer_fields() {
    let id = RenderHistoryId::try_new(7).expect("positive id");
    let entry = to_viewer_entry(RecentRenderRecord {
        id,
        title: "Named diff".into(),
        repo_name: "git-tools".into(),
        kind: "merge-diff".into(),
        range_label: "main...feature".into(),
        rendered_at: "2026-07-11T10:00:00Z".into(),
        recipe_json: "{}".into(),
    });

    assert_eq!(entry.id(), id);
    assert_eq!(entry.title(), "Named diff");
    assert_eq!(entry.repo_name(), "git-tools");
    assert_eq!(entry.kind(), "merge-diff");
    assert_eq!(entry.range_label(), "main...feature");
    assert_eq!(entry.rendered_at(), "2026-07-11T10:00:00Z");
}

#[test]
fn document_does_not_drain_pending_and_pending_drains_every_batch_atomically() {
    let app = ViewerApp::new(
        test_support::fake_mediator(),
        Path::new("/data").into(),
        1024,
    );
    app.pending()
        .push(OpenRecipes {
            batch_id: "first".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: Vec::new(),
        })
        .expect("queue first batch");
    app.pending()
        .push(OpenRecipes {
            batch_id: "second".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: Vec::new(),
        })
        .expect("queue second batch");

    let document = serve_app(&app, request("/"));
    assert_eq!(document.status(), StatusCode::OK);
    assert_eq!(document.headers()["Cache-Control"], "no-store");
    assert!(String::from_utf8_lossy(document.body()).contains("<!DOCTYPE html>"));

    let pending = serve_app(&app, request("/pending"));
    let html = String::from_utf8_lossy(pending.body());
    assert_eq!(pending.status(), StatusCode::OK);
    assert_eq!(pending.headers()["Cache-Control"], "no-store");
    assert!(html.starts_with("<nav id=\"viewer-tabs\""));
    assert!(html.contains("hx-swap-oob=\"outerHTML\""));
    assert!(app.pending().drain().is_empty());
}

#[test]
fn empty_pending_response_is_not_cached_before_a_later_batch_arrives() {
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            source(),
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        ),
        Path::new("/data").into(),
        128 * 1024 * 1024,
    );

    let empty = serve_app(&app, request("/pending"));
    assert_eq!(empty.headers()["Cache-Control"], "no-store");

    app.pending()
        .push(OpenRecipes {
            batch_id: "later".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![recipe()],
        })
        .expect("queue later batch");
    let populated = serve_app(&app, request("/pending"));

    assert_eq!(populated.headers()["Cache-Control"], "no-store");
    assert!(String::from_utf8_lossy(populated.body()).contains("class=\"layout"));
}

#[test]
fn pending_recipe_name_labels_the_opened_tab() {
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            source(),
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        ),
        Path::new("/data").into(),
        128 * 1024 * 1024,
    );
    let mut named = recipe();
    named.name = Some("Friendly diff".into());
    app.pending()
        .push(OpenRecipes {
            batch_id: "machine-batch-id".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![named],
        })
        .expect("queue named recipe");

    let response = serve_app(&app, request("/pending"));
    let html = String::from_utf8_lossy(response.body());

    assert_eq!(response.status(), StatusCode::OK);
    assert!(html.contains("Friendly diff"), "{html}");
    assert!(!html.contains("machine-batch-id"), "{html}");

    let repeated = serve_app(&app, request("/pending"));
    let repeated_html = String::from_utf8_lossy(repeated.body());
    assert_eq!(repeated.status(), StatusCode::OK);
    assert_eq!(
        repeated_html.matches("class=\"viewer-tab active\"").count(),
        1
    );
}

#[test]
fn live_batch_reuses_the_named_restored_tab_and_keeps_it_live() {
    let app_state = InMemoryAppStateStore::default();
    app_state
        .live_views
        .lock()
        .expect("live views lock")
        .push(LiveViewRecord {
            source_kind: "LocalRepo".into(),
            source_value: "/repo".into(),
            display_name: "Friendly live".into(),
            created_at: "2026-07-11T00:00:00Z".into(),
            last_opened_at: None,
        });
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            source(),
            app_state,
            FakeRepoProbe {
                result: RepoProbeResult::Repo {
                    top_level: "/repo".into(),
                },
            },
        ),
        Path::new("/data").into(),
        128 * 1024 * 1024,
    );
    let mut named = recipe();
    named.name = Some("Friendly live".into());
    app.pending()
        .push(OpenRecipes {
            batch_id: "forwarded-live".into(),
            kind: RecipeBatchKind::Live,
            recipes: vec![named],
        })
        .expect("queue live batch");

    assert_eq!(serve_app(&app, request("/")).status(), StatusCode::OK);
    assert_eq!(
        serve_app(&app, request("/pending")).status(),
        StatusCode::OK
    );

    let session = app.session.lock().expect("session lock");
    assert_eq!(session.tabs().len(), 1, "restoration and forward dedupe");
    let tab = session.tabs().next().expect("one live tab");
    assert_eq!(tab.tab.kind(), ViewerTabKind::Live);
    assert_eq!(tab.tab.label(), "Friendly live");
    assert_eq!(tab.batch_id, "forwarded-live");
}

#[test]
fn view_persists_options_and_close_returns_compound_state() {
    let app_state = InMemoryAppStateStore::default();
    let mediator = test_support::fake_mediator_with(
        source(),
        app_state.clone(),
        FakeRepoProbe {
            result: RepoProbeResult::Repo {
                top_level: "/repo".into(),
            },
        },
    );
    let app = ViewerApp::new(mediator, Path::new("/data").into(), 128 * 1024 * 1024);
    app.pending()
        .push(OpenRecipes {
            batch_id: "batch".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![recipe()],
        })
        .expect("queue batch");
    let opened = serve_app(&app, request("/pending"));
    assert_eq!(opened.status(), StatusCode::OK);

    let view = serve_app(&app, request("/tabs/1/view?layout=split&density=full"));
    let html = String::from_utf8_lossy(view.body());
    assert_eq!(view.status(), StatusCode::OK);
    assert!(html.starts_with("<section id=\"viewer-view\""));
    assert!(html.contains("id=\"viewer-tabs\" hx-swap-oob=\"outerHTML\""));
    let settings = app_state.settings.lock().expect("settings lock");
    assert_eq!(settings.get("layout").map(String::as_str), Some("split"));
    assert_eq!(settings.get("density").map(String::as_str), Some("full"));
    drop(settings);
    assert!({
        let mut session = app.session.lock().expect("session lock");
        let id = ViewerTabId::try_new(1).expect("positive id");
        let ticket = session.current_ticket(id).expect("current ticket");
        session
            .cached_fragment_if_current(
                ticket,
                RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
                domain::viewer::Theme::Dark,
            )
            .is_some()
    });

    let repeated = serve_app(&app, request("/tabs/1/view?layout=split&density=full"));
    assert_eq!(repeated.body(), view.body());

    assert_eq!(
        serve_app(&app, request("/settings?theme=light")).status(),
        StatusCode::NO_CONTENT
    );
    let light = serve_app(&app, request("/tabs/1/view?layout=split&density=full"));
    let light_html = String::from_utf8_lossy(light.body());
    assert!(light_html.contains("value=\"light\" data-viewer-theme=\"light\" checked"));
    assert!(!light_html.contains("value=\"dark\" data-viewer-theme=\"dark\" checked"));

    let closed = serve_app(&app, request("/tabs/1/close"));
    let html = String::from_utf8_lossy(closed.body());
    assert!(html.starts_with("<nav id=\"viewer-tabs\""));
    assert!(html.contains("id=\"viewer-view\" hx-swap-oob=\"outerHTML\""));
}

#[test]
fn stable_history_id_reopens_recipe_and_missing_tabs_are_not_found() {
    let app_state = InMemoryAppStateStore::default();
    let id = RenderHistoryId::try_new(77).expect("positive id");
    app_state
        .renders
        .lock()
        .expect("renders lock")
        .push(RecentRenderRecord {
            id,
            recipe_json: serde_json::to_string(&recipe()).expect("recipe serializes"),
            title: "Historic".into(),
            repo_name: "repo".into(),
            kind: "diff".into(),
            range_label: "main..HEAD".into(),
            rendered_at: "2026-07-11T00:00:00Z".into(),
        });
    let mediator = test_support::fake_mediator_with(source(), app_state, FakeRepoProbe::default());
    let app = ViewerApp::new(mediator, Path::new("/data").into(), 128 * 1024 * 1024);

    let reopened = serve_app(&app, request("/history/77/open"));
    let html = String::from_utf8_lossy(reopened.body());
    assert_eq!(reopened.status(), StatusCode::OK);
    assert!(html.contains("Historic") || html.contains("feature"));
    assert_eq!(
        serve_app(&app, request("/tabs/99/activate")).status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        serve_app(&app, request("/tabs/99/close")).status(),
        StatusCode::NOT_FOUND
    );
    let expected = ViewerTabId::try_new(1).expect("positive id");
    assert_eq!(app.active_tab(), Some(expected));
}

#[test]
fn restored_broken_live_source_and_error_details_are_escaped() {
    let app_state = InMemoryAppStateStore::default();
    app_state
        .live_views
        .lock()
        .expect("live views lock")
        .push(LiveViewRecord {
            source_kind: "LocalRepo".into(),
            source_value: "/gone/<script>".into(),
            display_name: "gone".into(),
            created_at: "2026-07-11T00:00:00Z".into(),
            last_opened_at: None,
        });
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            FakeDiffSource::default(),
            app_state,
            FakeRepoProbe::default(),
        ),
        Path::new("/data").into(),
        1024,
    );

    let response = serve_app(&app, request("/"));
    let html = String::from_utf8_lossy(response.body());
    assert_eq!(response.status(), StatusCode::OK);
    assert!(html.contains("This diff cannot be opened"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("/gone/<script>"));
}

#[test]
fn settings_methods_and_content_types_are_explicit() {
    let app_state = InMemoryAppStateStore::default();
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            FakeDiffSource::default(),
            app_state.clone(),
            FakeRepoProbe::default(),
        ),
        Path::new("/data").into(),
        1024,
    );

    let response = serve_app(&app, request("/settings?theme=hearth"));
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some(TEXT_CONTENT_TYPE)
    );
    assert_eq!(
        app_state
            .settings
            .lock()
            .expect("settings lock")
            .get("theme")
            .map(String::as_str),
        Some("hearth")
    );

    let post = Request::builder()
        .method(Method::POST)
        .uri(format!("{}history", protocol_config::APP_URL))
        .body(Vec::new())
        .expect("request builds");
    let response = serve_app(&app, post);
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some(TEXT_CONTENT_TYPE)
    );

    let document = serve_app(&app, request("/"));
    assert_eq!(
        document
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some(HTML_CONTENT_TYPE)
    );
}

#[test]
fn activating_a_deferred_restored_tab_computes_it_on_demand() {
    let app_state = InMemoryAppStateStore::default();
    app_state
        .live_views
        .lock()
        .expect("live views lock")
        .extend(
            ["/first", "/second"]
                .into_iter()
                .map(|path| LiveViewRecord {
                    source_kind: "LocalRepo".into(),
                    source_value: path.into(),
                    display_name: path.into(),
                    created_at: "2026-07-11T00:00:00Z".into(),
                    last_opened_at: None,
                }),
        );
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            source(),
            app_state,
            FakeRepoProbe {
                result: RepoProbeResult::Repo {
                    top_level: "/repo".into(),
                },
            },
        ),
        Path::new("/data").into(),
        128 * 1024 * 1024,
    );
    assert_eq!(serve_app(&app, request("/")).status(), StatusCode::OK);

    let activated = serve_app(&app, request("/tabs/1/activate"));
    let html = String::from_utf8_lossy(activated.body());
    assert_eq!(activated.status(), StatusCode::OK);
    assert!(html.starts_with("<section id=\"viewer-view\""));
    assert!(html.contains("diff-unified"));
}

#[test]
fn poisoned_session_lock_returns_sanitized_target_shaped_errors() {
    let app = ViewerApp::new(
        test_support::fake_mediator(),
        Path::new("/data").into(),
        1024,
    );
    let session = Arc::clone(&app.session);
    let _ = catch_unwind(AssertUnwindSafe(move || {
        let _guard = session.lock().expect("initial session lock");
        panic!("poison route session");
    }));

    for (route, root) in [
        ("/tabs/1/activate", "<section id=\"viewer-view\""),
        ("/tabs/1/close", "<nav id=\"viewer-tabs\""),
    ] {
        let response = serve_app(&app, request(route));
        let html = String::from_utf8_lossy(response.body());
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(html.starts_with(root), "{html}");
        assert!(html.contains("Please retry"));
        assert!(!html.contains("poison"));
        assert!(!html.contains("route session"));
    }
}

#[test]
fn pending_processing_preserves_fifo_failure_remainder_for_retry() {
    fn named(path: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(path.into()),
            op: RecipeOp::SquashPreview { pinned: None },
            name: None,
        }
    }
    let batches = vec![
        OpenRecipes {
            batch_id: "first".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![named("/one"), named("/fail"), named("/three")],
        },
        OpenRecipes {
            batch_id: "second".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![named("/four")],
        },
    ];
    let mut opened = Vec::new();
    let failure = process_pending(batches, |recipe, batch, _kind| {
        let path = recipe.cwd().display().to_string();
        opened.push((batch.to_string(), path.clone()));
        if path == "/fail" {
            Err(String::from("boom"))
        } else {
            Ok(path)
        }
    })
    .expect_err("middle recipe fails");

    assert_eq!(
        opened,
        vec![
            ("first".into(), "/one".into()),
            ("first".into(), "/fail".into())
        ]
    );
    assert_eq!(
        failure
            .remainder
            .iter()
            .map(|batch| (
                batch.batch_id.clone(),
                batch.recipes.iter().map(Recipe::cwd).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "first".into(),
                vec![PathBuf::from("/fail"), PathBuf::from("/three")]
            ),
            ("second".into(), vec![PathBuf::from("/four")]),
        ]
    );
    let mut retried = Vec::new();
    process_pending(failure.remainder, |recipe, batch, _kind| {
        retried.push((batch.to_string(), recipe.cwd()));
        Ok::<(), String>(())
    })
    .expect("retry succeeds");
    assert_eq!(
        retried,
        vec![
            ("first".into(), PathBuf::from("/fail")),
            ("first".into(), PathBuf::from("/three")),
            ("second".into(), PathBuf::from("/four"))
        ]
    );
}

#[test]
fn conflict_and_internal_errors_keep_target_roots_and_hide_details() {
    for (target, error, status, root) in [
        (
            ErrorTarget::View,
            RouteError::Conflict,
            StatusCode::CONFLICT,
            "<section id=\"viewer-view\"",
        ),
        (
            ErrorTarget::Tabs,
            RouteError::Internal("sqlite /secret/path".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
            "<nav id=\"viewer-tabs\"",
        ),
    ] {
        let response = error_response(target, &error);
        let html = String::from_utf8_lossy(response.body());
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["X-GTL-Recovery"], "true");
        assert_eq!(response.headers()["HX-Reswap"], "outerHTML");
        assert!(html.starts_with(root));
        assert!(!html.contains("sqlite"));
        assert!(!html.contains("/secret/path"));
    }
}

#[test]
fn settings_failure_returns_an_unmarked_sanitized_document_error() {
    let app_state = InMemoryAppStateStore::default();
    *app_state
        .set_setting_error
        .lock()
        .expect("setting error lock") = Some("secret settings persistence failure".into());
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            FakeDiffSource::default(),
            app_state,
            FakeRepoProbe::default(),
        ),
        Path::new("/data").into(),
        1024,
    );
    let response = serve_app(&app, request("/settings?theme=light"));
    let html = String::from_utf8_lossy(response.body());

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("The viewer could not complete this request"));
    assert!(!html.contains("secret settings persistence failure"));
    assert!(response.headers().get("X-GTL-Recovery").is_none());
    assert!(response.headers().get("HX-Reswap").is_none());
}

#[test]
fn compute_failure_publishes_sanitized_tab_and_does_not_block_later_batches() {
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            source(),
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        ),
        Path::new("/data").into(),
        128 * 1024 * 1024,
    );
    let failed = Recipe {
        source: RecipeSource::LocalRepo("/sentinel/sql/path".into()),
        op: RecipeOp::Diff {
            target: RecipeTarget::Base {
                rev: "missing-secret-revision".into(),
            },
        },
        name: Some("Failed first".into()),
    };
    let mut second = recipe();
    second.source = RecipeSource::LocalRepo("/second".into());
    second.name = Some("Second succeeds".into());
    let mut third = recipe();
    third.source = RecipeSource::LocalRepo("/third".into());
    third.name = Some("Third succeeds".into());
    app.pending()
        .push(OpenRecipes {
            batch_id: "first".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![failed, second],
        })
        .expect("queue first batch");
    app.pending()
        .push(OpenRecipes {
            batch_id: "second".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![third],
        })
        .expect("queue second batch");

    let response = serve_app(&app, request("/pending"));
    let html = String::from_utf8_lossy(response.body());

    assert_eq!(response.status(), StatusCode::OK);
    assert!(html.starts_with("<nav id=\"viewer-tabs\""));
    let failed_index = html.find("Failed first").expect("failed tab label");
    let second_index = html.find("Second succeeds").expect("second tab label");
    let third_index = html.find("Third succeeds").expect("third tab label");
    assert!(
        failed_index < second_index && second_index < third_index,
        "{html}"
    );
    assert!(!html.contains("missing-secret-revision"));
    assert!(!html.contains("sentinel"));
    assert!(!html.contains("sql/path"));
    assert!(app.pending().drain().is_empty());
    let session = app.session.lock().expect("session lock");
    let failed_tab = session.tabs().next().expect("failed tab remains open");
    assert_eq!(failed_tab.tab.label(), "Failed first");
    assert_eq!(
        failed_tab.tab.state(),
        &domain::viewer::ViewerTabState::Error {
            reason: "The diff could not be rendered. Please retry.".into(),
        }
    );
}

#[test]
fn retryable_pending_failure_requeues_the_exact_fifo_remainder() {
    let app = ViewerApp::new(
        test_support::fake_mediator_with(
            source(),
            InMemoryAppStateStore::default(),
            FakeRepoProbe::default(),
        ),
        Path::new("/data").into(),
        128 * 1024 * 1024,
    );
    let mut first = recipe();
    first.name = Some("first".into());
    let mut second = recipe();
    second.source = RecipeSource::LocalRepo("/second".into());
    second.name = Some("second".into());
    let queued = vec![
        OpenRecipes {
            batch_id: "one".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![first],
        },
        OpenRecipes {
            batch_id: "two".into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: vec![second],
        },
    ];
    for batch in queued.clone() {
        app.pending().push(batch).expect("queue batch");
    }
    let session = Arc::clone(&app.session);
    let _ = catch_unwind(AssertUnwindSafe(move || {
        let _guard = session.lock().expect("initial session lock");
        panic!("poison pending session");
    }));

    let response = serve_app(&app, request("/pending"));

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(app.pending().drain(), queued);
}
