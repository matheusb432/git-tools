use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use application::{
    history::list_recent::list as list_recent_renders,
    live_views::{list as list_live_views, save as save_live_view},
    ports::RecentRenderRecord,
    viewer::{RenderHistoryId, ViewerTabKind},
};
use gtl_recipe::{
    OpenRecipes, PinnedRange, Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget,
};
use infra::user_config::TomlSettingsStore;
use tauri::http::{Method, Request, StatusCode};
use tempfile::TempDir;

use super::{
    ErrorTarget, HTML_CONTENT_TYPE, PendingRecipeOutcome, RouteError, TEXT_CONTENT_TYPE, ViewerApp,
    error_response, history::to_viewer_entry, process_pending, serve_app,
};
use crate::protocol_config;

struct Fixture {
    _temp: TempDir,
    app: ViewerApp,
    config_path: PathBuf,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        Self::with_config(None)
    }

    fn with_config(config_toml: Option<&str>) -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo dir");
        init_repo(&repo);
        let config_path = temp.path().join("config.toml");
        if let Some(raw) = config_toml {
            std::fs::write(&config_path, raw).expect("write config");
        }
        let app = ViewerApp::new(
            temp.path().join("data"),
            TomlSettingsStore::new(Some(config_path.clone())),
            128 * 1024 * 1024,
        );
        Self {
            _temp: temp,
            app,
            config_path,
            repo,
        }
    }

    fn recipe(&self) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(self.repo.clone()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn enqueue(&self, batch_id: &str, kind: RecipeBatchKind, recipes: Vec<Recipe>) {
        self.app
            .pending()
            .push(OpenRecipes {
                batch_id: batch_id.into(),
                kind,
                recipes,
            })
            .expect("enqueue recipes");
    }
}

fn init_repo(repo: &Path) {
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["config", "user.email", "test@example.com"]);
    git(repo, &["config", "user.name", "Test User"]);
    std::fs::write(repo.join("f.txt"), "base\n").expect("base file");
    git(repo, &["add", "f.txt"]);
    git(repo, &["commit", "-qm", "initial"]);
    git(repo, &["switch", "-qc", "feature"]);
    std::fs::write(repo.join("f.txt"), "feature\n").expect("feature file");
    git(repo, &["add", "f.txt"]);
    git(repo, &["commit", "-qm", "feature"]);
    git(repo, &["branch", "--set-upstream-to=main", "feature"]);
}

fn git(repo: &Path, args: &[&str]) {
    git_stdout(repo, args);
}

fn git_stdout(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .into()
}

fn request(path: &str) -> Request<Vec<u8>> {
    request_with_method(Method::GET, path)
}

fn request_with_method(method: Method, path: &str) -> Request<Vec<u8>> {
    Request::builder()
        .method(method)
        .uri(format!(
            "{}{}",
            protocol_config::APP_URL,
            path.trim_start_matches('/')
        ))
        .body(Vec::new())
        .expect("request")
}

#[test]
fn recent_render_mapping_preserves_viewer_fields() {
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
fn document_keeps_pending_work_and_pending_opens_the_recipe() {
    let fixture = Fixture::new();
    fixture.enqueue("batch", RecipeBatchKind::Snapshot, vec![fixture.recipe()]);

    let document = serve_app(&fixture.app, request("/"));
    assert_eq!(document.status(), StatusCode::OK);
    assert!(fixture.app.active_tab().is_none());

    let pending = serve_app(&fixture.app, request("/pending"));
    assert_eq!(pending.status(), StatusCode::OK);
    assert_eq!(pending.headers()["content-type"], HTML_CONTENT_TYPE);
    assert!(fixture.app.active_tab().is_some());
    let html = String::from_utf8(pending.into_body()).expect("html");
    assert!(html.contains("viewer-tabs"));
    assert!(html.contains("f.txt"));
}

#[test]
fn pending_snapshot_skips_render_the_empty_viewer_and_escaped_toast() {
    let fixture = Fixture::new();
    git(&fixture.repo, &["switch", "-q", "main"]);
    let mut recipe = fixture.recipe();
    recipe.name = Some("<script>empty snapshot</script>".into());
    let head = git_stdout(&fixture.repo, &["rev-parse", "HEAD"]);
    recipe.op = RecipeOp::Diff {
        target: RecipeTarget::Unpushed {
            pinned: Some(PinnedRange {
                base: head.clone(),
                head,
            }),
        },
    };
    fixture.enqueue("batch", RecipeBatchKind::Snapshot, vec![recipe]);

    let response = serve_app(&fixture.app, request("/pending"));

    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(response.into_body()).expect("html");
    assert!(!html.contains("class=\"viewer-tab\""));
    assert!(html.contains("viewer-status-empty"));
    assert!(html.contains("data-viewer-toast"));
    assert!(html.contains("&lt;script&gt;empty snapshot&lt;/script&gt;"));
    assert!(!html.contains("<script>empty snapshot</script>"));
}

#[test]
fn configured_exclusions_hide_files_and_render_the_chip() {
    let fixture = Fixture::with_config(Some("[diff.exclude]\nrepo = [\"md\"]\n"));
    std::fs::write(fixture.repo.join("notes.md"), "plan\n").expect("notes file");
    git(&fixture.repo, &["add", "notes.md"]);
    git(&fixture.repo, &["commit", "-qm", "docs: notes"]);
    fixture.enqueue("batch", RecipeBatchKind::Snapshot, vec![fixture.recipe()]);
    serve_app(&fixture.app, request("/"));

    let pending = serve_app(&fixture.app, request("/pending"));

    assert_eq!(pending.status(), StatusCode::OK);
    let html = String::from_utf8(pending.into_body()).expect("html");
    assert!(html.contains(r#"data-path="f.txt""#));
    assert!(
        !html.contains(r#"data-path="notes.md""#),
        "excluded file must not render a file block"
    );
    assert!(
        html.contains(r#"<span class="excl-chip""#),
        "exclusion chip missing"
    );
    assert!(html.contains("1 file hidden · md"));
    assert!(
        html.contains("notes.md"),
        "the chip tooltip still names the hidden path"
    );
}

#[test]
fn settings_persist_through_toml_and_render_in_the_document() {
    let fixture = Fixture::with_config(Some("[push]\nconfirm = false\n"));
    assert_eq!(
        serve_app(&fixture.app, request("/settings?theme=light")).status(),
        StatusCode::NO_CONTENT
    );

    let raw = std::fs::read_to_string(&fixture.config_path).expect("settings TOML");
    let document = toml::from_str::<toml::Value>(&raw).expect("valid TOML");
    assert_eq!(document["theme"].as_str(), Some("light"));
    assert_eq!(document["push"]["confirm"].as_bool(), Some(false));

    let response = serve_app(&fixture.app, request("/"));
    let html = String::from_utf8(response.into_body()).expect("html");
    assert!(html.contains("data-theme=\"light\""), "{html}");
}

#[test]
fn layout_and_density_restore_from_toml_before_rendering() {
    let fixture = Fixture::with_config(Some("layout = \"split\"\ndensity = \"full\"\n"));

    let response = serve_app(&fixture.app, request("/"));
    let html = String::from_utf8(response.into_body()).expect("html");

    assert!(
        html.contains("data-theme=\"dark\" data-diff-layout=\"split\" data-diff-full=\"on\""),
        "{html}"
    );
}

#[test]
fn saved_live_view_restores_and_deletes_through_real_persistence() {
    let fixture = Fixture::new();
    save_live_view::execute(
        save_live_view::SaveLiveView {
            data_root: (*fixture.app.data_root).clone(),
            path: fixture.repo.clone(),
        },
        &fixture.app.probe,
        &fixture.app.app_state,
        &fixture.app.clock,
    )
    .expect("save live view");

    let document = serve_app(&fixture.app, request("/"));
    assert_eq!(document.status(), StatusCode::OK);
    let tab = fixture.app.active_tab().expect("restored tab");
    let kind = fixture
        .app
        .session
        .lock()
        .expect("session")
        .tab(tab)
        .expect("tab")
        .tab
        .kind();
    assert_eq!(kind, ViewerTabKind::Live);

    let deleted = serve_app(
        &fixture.app,
        request_with_method(Method::DELETE, &format!("/tabs/{tab}/live-view")),
    );
    assert_eq!(deleted.status(), StatusCode::OK);
    let remaining = list_live_views::execute(
        list_live_views::ListLiveViews {
            data_root: (*fixture.app.data_root).clone(),
        },
        &fixture.app.app_state,
    )
    .expect("list live views");
    assert!(remaining.views.is_empty());
}

#[test]
fn stable_history_id_reopens_a_recorded_recipe() {
    let fixture = Fixture::new();
    fixture.enqueue("batch", RecipeBatchKind::Snapshot, vec![fixture.recipe()]);
    assert_eq!(
        serve_app(&fixture.app, request("/pending")).status(),
        StatusCode::OK
    );
    let history = list_recent_renders::execute(
        list_recent_renders::ListRecentRenders {
            data_root: (*fixture.app.data_root).clone(),
        },
        &fixture.app.app_state,
    )
    .expect("list history");
    let id = history.entries.first().expect("history entry").id;

    let response = serve_app(&fixture.app, request(&format!("/history/{id}/open")));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], HTML_CONTENT_TYPE);
}

#[test]
fn missing_recipe_becomes_a_sanitized_error_tab_without_blocking_the_batch() {
    let fixture = Fixture::new();
    let mut missing = fixture.recipe();
    missing.source = RecipeSource::LocalRepo(fixture.repo.join("missing"));
    let mut valid = fixture.recipe();
    valid.name = Some("valid after failure".into());
    fixture.enqueue("batch", RecipeBatchKind::Snapshot, vec![missing, valid]);

    let response = serve_app(&fixture.app, request("/pending"));
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(response.into_body()).expect("html");
    assert!(html.contains("valid after failure"));
    assert!(!html.contains("not a git repository"));
}

#[test]
fn view_persists_options_and_close_returns_compound_state() {
    let fixture = Fixture::new();
    fixture.enqueue("batch", RecipeBatchKind::Snapshot, vec![fixture.recipe()]);
    serve_app(&fixture.app, request("/pending"));
    let tab = fixture.app.active_tab().expect("active tab");

    let view = serve_app(
        &fixture.app,
        request(&format!("/tabs/{tab}/view?layout=split&density=full")),
    );
    assert_eq!(view.status(), StatusCode::OK);
    let document = serve_app(&fixture.app, request("/"));
    let html = String::from_utf8(document.into_body()).expect("html");
    assert!(html.contains("data-diff-layout=\"split\""));
    assert!(html.contains("data-diff-full=\"on\""));

    let close = serve_app(&fixture.app, request(&format!("/tabs/{tab}/close")));
    assert_eq!(close.status(), StatusCode::OK);
    assert!(fixture.app.active_tab().is_none());
}

#[test]
fn methods_and_content_types_are_explicit() {
    let fixture = Fixture::new();
    let document = serve_app(&fixture.app, request("/"));
    assert_eq!(document.headers()["content-type"], HTML_CONTENT_TYPE);

    let wrong_method = serve_app(
        &fixture.app,
        request_with_method(Method::DELETE, "/settings?theme=dark"),
    );
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(wrong_method.headers()["content-type"], TEXT_CONTENT_TYPE);
}

#[test]
fn poisoned_session_lock_returns_sanitized_target_shaped_errors() {
    let fixture = Fixture::new();
    let session = Arc::clone(&fixture.app.session);
    let _ = catch_unwind(AssertUnwindSafe(move || {
        let _guard = session.lock().expect("initial lock");
        panic!("poison route session");
    }));

    let response = serve_app(&fixture.app, request("/tabs/1/activate"));
    let html = String::from_utf8_lossy(response.body());
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(html.starts_with("<section id=\"viewer-view\""));
    assert!(html.contains("Please retry"));
    assert!(!html.contains("poison"));
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
            Ok(PendingRecipeOutcome::Opened(path))
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
    assert_eq!(failure.remainder[0].recipes.len(), 2);
    assert_eq!(
        failure.remainder[0].recipes[0].cwd(),
        PathBuf::from("/fail")
    );
    assert_eq!(failure.remainder[1].batch_id, "second");
}

#[test]
fn pending_processing_preserves_opened_and_skipped_results_independently() {
    fn named(path: &str) -> Recipe {
        Recipe {
            source: RecipeSource::LocalRepo(path.into()),
            op: RecipeOp::SquashPreview { pinned: None },
            name: None,
        }
    }
    let batches = vec![OpenRecipes {
        batch_id: "batch".into(),
        kind: RecipeBatchKind::Snapshot,
        recipes: vec![named("/one"), named("/skip"), named("/three")],
    }];

    let processed = process_pending(batches, |recipe, _batch, _kind| {
        let path = recipe.cwd().display().to_string();
        Ok::<_, String>(if path == "/skip" {
            PendingRecipeOutcome::Skipped("skip label".into())
        } else {
            PendingRecipeOutcome::Opened(path)
        })
    })
    .expect("batch succeeds");

    assert_eq!(processed.latest_opened.as_deref(), Some("/three"));
    assert_eq!(processed.skipped_labels, ["skip label"]);
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
