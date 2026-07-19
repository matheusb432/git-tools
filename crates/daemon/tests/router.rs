//! Router-level tests exercising the concrete daemon state through real temporary boundaries.

use std::{path::Path, process::Command, time::Duration};

use application::diffs::{
    DiffTargetRequest, RepoRef, render_diff::RenderDiff, render_diff_all::RenderDiffAll,
    render_diff_subrepos::RenderDiffSubrepos, render_merge_diff::RenderMergeDiff,
    render_squash_preview::RenderSquashPreview,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use daemon::{lifecycle::ExeIdentity, state::DaemonState};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::watch;
use tower::ServiceExt as _;

struct Fixture {
    _temp: TempDir,
    app: Router,
    shutdown_rx: watch::Receiver<bool>,
    repo: std::path::PathBuf,
    store: std::path::PathBuf,
    data: std::path::PathBuf,
    manifest: std::path::PathBuf,
}

impl Fixture {
    fn new(with_feature_commit: bool) -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo = temp.path().join("repo");
        let store = temp.path().join("store");
        let data = temp.path().join("data");
        let manifest = temp.path().join("repos.toml");
        std::fs::create_dir_all(&repo).expect("repo dir");
        init_repo(&repo, with_feature_commit);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let app_state = infra::app_state::SqliteAppState::open(&data).expect("open app state");
        let app = daemon::state::router(DaemonState::new(
            ExeIdentity {
                exe_len: 4242,
                exe_modified_ms: 111,
            },
            "9.9.9",
            4242,
            shutdown_tx,
            app_state,
            infra::user_config::TomlSettingsStore::new(None),
        ));
        Self {
            _temp: temp,
            app,
            shutdown_rx,
            repo,
            store,
            data,
            manifest,
        }
    }

    async fn post(&self, uri: &str, body: Value) -> axum::response::Response {
        post(self.app.clone(), uri, &body.to_string()).await
    }
}

fn init_repo(repo: &Path, with_feature_commit: bool) {
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["config", "user.email", "test@example.com"]);
    git(repo, &["config", "user.name", "Test User"]);
    std::fs::write(repo.join("f.txt"), "base\n").expect("base file");
    git(repo, &["add", "f.txt"]);
    git(repo, &["commit", "-qm", "initial"]);
    git(repo, &["switch", "-qc", "feature"]);
    if with_feature_commit {
        std::fs::write(repo.join("f.txt"), "feature\n").expect("feature file");
        git(repo, &["add", "f.txt"]);
        git(repo, &["commit", "-qm", "feature"]);
    }
    git(repo, &["branch", "--set-upstream-to=main", "feature"]);
}

fn git(repo: &Path, args: &[&str]) {
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
}

async fn post(app: Router, uri: &str, body: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request"),
    )
    .await
    .expect("response")
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json body")
}

fn assert_ok_artifact(json: &Value) {
    assert_eq!(json["outcome"], "ok");
    let artifact = json["data"]["artifact"].as_str().expect("artifact path");
    assert!(!artifact.is_empty());
    assert!(
        Path::new(artifact).is_file(),
        "artifact should exist: {artifact}"
    );
}

#[test]
fn application_diff_requests_preserve_the_route_json_contract() {
    let render = RenderDiff {
        cwd: "/repo".into(),
        store_root: "/store".into(),
        target: DiffTargetRequest::Base { rev: "main".into() },
        name: Some("review".into()),
    };
    let render_json = json!({
        "cwd": "/repo",
        "store_root": "/store",
        "target": {"kind": "base", "rev": "main"},
        "name": "review"
    });
    assert_eq!(serde_json::to_value(&render).unwrap(), render_json);
    assert_eq!(
        serde_json::from_value::<RenderDiff>(render_json).unwrap(),
        render
    );

    let merge = RenderMergeDiff {
        cwd: "/repo".into(),
        store_root: "/store".into(),
        base: Some("main".into()),
    };
    let merge_json = json!({"cwd": "/repo", "store_root": "/store", "base": "main"});
    assert_eq!(serde_json::to_value(&merge).unwrap(), merge_json);
    assert_eq!(
        serde_json::from_value::<RenderMergeDiff>(merge_json).unwrap(),
        merge
    );

    let squash = RenderSquashPreview {
        cwd: "/repo".into(),
        store_root: "/store".into(),
    };
    let squash_json = json!({"cwd": "/repo", "store_root": "/store"});
    assert_eq!(serde_json::to_value(&squash).unwrap(), squash_json);
    assert_eq!(
        serde_json::from_value::<RenderSquashPreview>(squash_json).unwrap(),
        squash
    );

    let repo = RepoRef {
        top: "/repo".into(),
        label: "repo".into(),
    };
    let subrepos = RenderDiffSubrepos {
        store_root: "/store".into(),
        root: "/root".into(),
        target: DiffTargetRequest::Unpushed,
        repos: vec![repo.clone()],
    };
    let subrepos_json = json!({
        "store_root": "/store",
        "root": "/root",
        "target": {"kind": "unpushed"},
        "repos": [{"top": "/repo", "label": "repo"}]
    });
    assert_eq!(serde_json::to_value(&subrepos).unwrap(), subrepos_json);
    assert_eq!(
        serde_json::from_value::<RenderDiffSubrepos>(subrepos_json).unwrap(),
        subrepos
    );

    let all = RenderDiffAll {
        store_root: "/store".into(),
        root: "/root".into(),
        repos: vec![repo],
    };
    let all_json = json!({
        "store_root": "/store",
        "root": "/root",
        "repos": [{"top": "/repo", "label": "repo"}]
    });
    assert_eq!(serde_json::to_value(&all).unwrap(), all_json);
    assert_eq!(
        serde_json::from_value::<RenderDiffAll>(all_json).unwrap(),
        all
    );
}

#[tokio::test]
async fn health_returns_startup_identity() {
    let fixture = Fixture::new(false);
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["pid"], 4242);
    assert_eq!(json["exe_len"], 4242);
    assert_eq!(json["exe_modified_ms"], 111);
}

#[tokio::test]
async fn shutdown_returns_202_and_flips_the_watch() {
    let mut fixture = Fixture::new(false);
    assert!(!*fixture.shutdown_rx.borrow());

    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shutdown")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    fixture
        .shutdown_rx
        .changed()
        .await
        .expect("shutdown change");
    assert!(*fixture.shutdown_rx.borrow());
}

#[tokio::test]
async fn render_with_a_bad_target_is_a_400_error_envelope() {
    let fixture = Fixture::new(false);
    let response = fixture
        .post(
            "/diffs/render",
            json!({"cwd": fixture.repo, "store_root": fixture.store, "target": {"kind": "last", "count": 0}}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["notes"][0]["level"], "error");
    assert_eq!(json["notes"][0]["text"], "last count must be >= 1");
}

#[tokio::test]
async fn subrepos_with_a_bad_target_is_a_400_error_envelope() {
    let fixture = Fixture::new(false);
    let response = fixture
        .post(
            "/diffs/subrepos",
            json!({
                "store_root": fixture.store,
                "root": fixture.repo,
                "target": {"kind": "last", "count": 0},
                "repos": []
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["notes"][0]["text"], "last count must be >= 1");
}

#[tokio::test]
async fn malformed_render_json_is_a_400_error_envelope() {
    let fixture = Fixture::new(false);
    let response = post(fixture.app.clone(), "/diffs/render", r#"{"target":42}"#).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["notes"][0]["level"], "error");
    assert!(
        json["notes"][0]["text"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
}

#[tokio::test]
async fn render_against_a_non_repo_is_a_500_error_envelope() {
    let fixture = Fixture::new(false);
    let response = fixture
        .post(
            "/diffs/render",
            json!({"cwd": fixture.store, "store_root": fixture.store, "target": {"kind": "unpushed"}}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["notes"][0]["level"], "error");
    let error_text = json["notes"][0]["text"].as_str().expect("error text");
    assert!(!error_text.is_empty(), "{json}");
}

#[tokio::test]
async fn render_happy_path_is_a_200_ok_envelope_with_artifact_and_notes() {
    let fixture = Fixture::new(true);
    let response = fixture
        .post(
            "/diffs/render",
            json!({"cwd": fixture.repo, "store_root": fixture.store, "target": {"kind": "unpushed"}}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_ok_artifact(&json);
    assert!(json["notes"].as_array().expect("notes").iter().any(|note| {
        note["text"]
            .as_str()
            .is_some_and(|text| text.contains("diff-preview:"))
    }));
}

#[tokio::test]
async fn merge_happy_path_is_a_200_ok_envelope_with_artifact() {
    let fixture = Fixture::new(true);
    let response = fixture
        .post(
            "/diffs/merge",
            json!({"cwd": fixture.repo, "store_root": fixture.store}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_ok_artifact(&body_json(response).await);
}

#[tokio::test]
async fn squash_preview_happy_path_is_a_200_ok_envelope_with_artifact() {
    let fixture = Fixture::new(true);
    let response = fixture
        .post(
            "/diffs/squash-preview",
            json!({"cwd": fixture.repo, "store_root": fixture.store}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_ok_artifact(&body_json(response).await);
}

#[tokio::test]
async fn all_happy_path_is_a_200_ok_envelope_with_artifact() {
    let fixture = Fixture::new(true);
    let response = fixture
        .post(
            "/diffs/all",
            json!({"store_root": fixture.store, "root": fixture.repo, "repos": [{"top": fixture.repo, "label": "repo"}]}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_ok_artifact(&body_json(response).await);
}

#[tokio::test]
async fn subrepos_happy_path_is_a_200_ok_envelope_with_artifact() {
    let fixture = Fixture::new(true);
    let response = fixture
        .post(
            "/diffs/subrepos",
            json!({"store_root": fixture.store, "root": fixture.repo, "target": {"kind": "unpushed"}, "repos": [{"top": fixture.repo, "label": "repo"}]}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_ok_artifact(&body_json(response).await);
}

#[tokio::test]
async fn subrepos_with_nothing_to_show_is_a_200_empty_envelope() {
    let fixture = Fixture::new(false);
    let response = fixture
        .post(
            "/diffs/subrepos",
            json!({"store_root": fixture.store, "root": fixture.repo, "target": {"kind": "unpushed"}, "repos": [{"top": fixture.repo, "label": "repo"}]}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "empty");
    assert!(json["data"].is_null());
}

async fn assert_managed_skip(uri: &str) {
    let fixture = Fixture::new(false);
    std::fs::write(&fixture.manifest, "[[repo]]\npath = 'missing'\n").expect("manifest");
    let response = fixture
        .post(
            uri,
            json!({"repos_file": fixture.manifest, "home_dir": fixture.data, "dry": false}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    let results = json["data"]["results"].as_array().expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["Status"], "skip");
}

#[tokio::test]
async fn managed_push_all_happy_path_is_a_200_ok_envelope_with_one_result() {
    assert_managed_skip("/managed/push-all").await;
}

#[tokio::test]
async fn managed_pull_all_happy_path_is_a_200_ok_envelope_with_one_result() {
    assert_managed_skip("/managed/pull-all").await;
}

#[tokio::test]
async fn managed_push_all_missing_manifest_is_a_500_error_envelope() {
    let fixture = Fixture::new(false);
    let response = fixture
        .post(
            "/managed/push-all",
            json!({
                "repos_file": fixture.data.join("missing.toml"),
                "home_dir": fixture.data,
                "dry": false
            }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["notes"][0]["level"], "error");
}

#[tokio::test]
async fn live_view_save_ignores_legacy_data_root_and_uses_daemon_store() {
    let fixture = Fixture::new(false);
    let redirected_root = fixture.data.with_file_name("redirected");
    let response = fixture
        .post(
            "/live-views/save",
            json!({"data_root": redirected_root, "path": fixture.repo}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    assert_eq!(json["data"]["source_kind"], "LocalRepo");
    assert_eq!(json["data"]["display_name"], "repo");
    assert_eq!(json["data"]["already_saved"], false);

    let connection =
        rusqlite::Connection::open(fixture.data.join("gtl.db")).expect("open daemon app database");
    let row_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM live_views \
             WHERE source_kind = 'LocalRepo' AND source_value = ?1",
            [fixture.repo.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .expect("count saved live view");
    assert_eq!(row_count, 1);
    assert!(!redirected_root.join("gtl.db").exists());
}

#[tokio::test]
async fn live_view_save_rejection_is_a_200_error_envelope_with_the_exact_message() {
    let fixture = Fixture::new(false);
    let gone = fixture.data.join("gone");
    let response = fixture
        .post("/live-views/save", json!({"path": gone}))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert!(json["data"].is_null());
    assert_eq!(json["notes"][0]["level"], "warn");
    assert_eq!(
        json["notes"][0]["text"],
        format!(
            "The git repo's directory at `{}` was not found.",
            gone.display()
        )
    );
}

#[tokio::test]
async fn live_view_save_with_a_malformed_body_is_a_400() {
    let fixture = Fixture::new(false);
    let response = post(fixture.app, "/live-views/save", "not json").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["notes"][0]["level"], "error");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_remains_responsive_while_live_view_save_waits_on_the_database() {
    let fixture = Fixture::new(false);
    let warm_response = fixture
        .post("/live-views/save", json!({"path": fixture.repo}))
        .await;
    assert_eq!(warm_response.status(), StatusCode::OK);

    let database_connection =
        rusqlite::Connection::open(fixture.data.join("gtl.db")).expect("open app database");
    database_connection
        .execute_batch("BEGIN IMMEDIATE")
        .expect("hold app database write lock");

    let save_router = fixture.app.clone();
    let save_body = json!({"path": fixture.repo}).to_string();
    let save_task =
        tokio::spawn(async move { post(save_router, "/live-views/save", &save_body).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !save_task.is_finished(),
        "the synchronous save should still be waiting on the database lock"
    );

    let health_response = tokio::time::timeout(
        Duration::from_millis(500),
        fixture.app.clone().oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        ),
    )
    .await
    .expect("health must not wait on the synchronous route")
    .expect("health response");
    assert_eq!(health_response.status(), StatusCode::OK);

    database_connection
        .execute_batch("ROLLBACK")
        .expect("release app database write lock");
    let save_response = tokio::time::timeout(Duration::from_secs(3), save_task)
        .await
        .expect("save completes after lock release")
        .expect("save task");
    assert_eq!(save_response.status(), StatusCode::OK);
}
