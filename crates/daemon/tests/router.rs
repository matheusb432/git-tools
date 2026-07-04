//! Router-level tests: drive the daemon's HTTP surface through `tower::oneshot`
//! with application fakes, asserting the lifecycle contract's status codes and
//! envelope shapes without binding a real socket.

use std::sync::{Arc, atomic::AtomicU64};

use application::{
    diffs::{
        render_diff::RenderDiffHandler, render_diff_all::RenderDiffAllHandler,
        render_diff_subrepos::RenderDiffSubreposHandler, render_merge_diff::RenderMergeDiffHandler,
        render_squash_preview::RenderSquashPreviewHandler,
    },
    managed::{pull_all::PullAllHandler, push_all::PushAllHandler},
    testing::{
        FakeDiffSource, FakeManagedManifest, FakePushLedger, FakeRemoteSync, FixedClock,
        InMemoryArtifactStore, StubRenderer,
    },
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use daemon::{
    lifecycle::ExeIdentity,
    state::{AppState, DaemonMediator, Shared, now_ms},
};
use http_body_util::BodyExt as _;
use tokio::sync::watch;
use tower::ServiceExt as _;

const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n";

type Fakes = (Router, watch::Receiver<bool>, Arc<Shared>);

fn app_with(source: FakeDiffSource) -> Fakes {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shared = Arc::new(Shared {
        identity: ExeIdentity {
            exe_len: 4242,
            exe_modified_ms: 111,
        },
        version: "9.9.9",
        pid: 4242,
        shutdown_tx,
        last_activity_ms: AtomicU64::new(now_ms()),
    });
    let mediator = DaemonMediator {
        render_diff: RenderDiffHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_merge_diff: RenderMergeDiffHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_squash_preview: RenderSquashPreviewHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_diff_subrepos: RenderDiffSubreposHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_diff_all: RenderDiffAllHandler {
            source,
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        push_all: PushAllHandler {
            remote: FakeRemoteSync::default(),
            manifest: FakeManagedManifest {
                repos: Vec::new(),
                error: None,
            },
            ledger: FakePushLedger::default(),
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        pull_all: PullAllHandler {
            remote: FakeRemoteSync::default(),
            manifest: FakeManagedManifest {
                repos: Vec::new(),
                error: None,
            },
        },
    };
    let router = daemon::state::router(AppState {
        mediator,
        shared: shared.clone(),
    });
    (router, shutdown_rx, shared)
}

fn app_with_managed(remote: FakeRemoteSync, manifest: FakeManagedManifest) -> Fakes {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shared = Arc::new(Shared {
        identity: ExeIdentity {
            exe_len: 4242,
            exe_modified_ms: 111,
        },
        version: "9.9.9",
        pid: 4242,
        shutdown_tx,
        last_activity_ms: AtomicU64::new(now_ms()),
    });
    let source = FakeDiffSource::default();
    let mediator = DaemonMediator {
        render_diff: RenderDiffHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_merge_diff: RenderMergeDiffHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_squash_preview: RenderSquashPreviewHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_diff_subrepos: RenderDiffSubreposHandler {
            source: source.clone(),
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        render_diff_all: RenderDiffAllHandler {
            source,
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        push_all: PushAllHandler {
            remote: remote.clone(),
            manifest: manifest.clone(),
            ledger: FakePushLedger::default(),
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        },
        pull_all: PullAllHandler { remote, manifest },
    };
    let router = daemon::state::router(AppState {
        mediator,
        shared: shared.clone(),
    });
    (router, shutdown_rx, shared)
}

/// A scripted happy-path diff source (mirrors the render_diff slice's happy test).
fn happy_source() -> FakeDiffSource {
    FakeDiffSource {
        top_level: Some("/repo".into()),
        branch: "feature".into(),
        upstream: Some("origin/main".into()),
        commits: vec![domain::diffs::Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }],
        diff_output: SINGLE_FILE_DIFF.into(),
        committed_at: "2026-07-02".into(),
        ..Default::default()
    }
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn health_returns_startup_identity() {
    let (app, _rx, _shared) = app_with(FakeDiffSource::default());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["pid"], 4242);
    assert_eq!(json["exe_len"], 4242);
    assert_eq!(json["exe_modified_ms"], 111);
}

#[tokio::test]
async fn shutdown_returns_202_and_flips_the_watch() {
    let (app, mut rx, _shared) = app_with(FakeDiffSource::default());
    assert!(!*rx.borrow());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/shutdown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    rx.changed().await.unwrap();
    assert!(*rx.borrow());
}

#[tokio::test]
async fn render_with_a_bad_target_is_a_400_error_envelope() {
    let (app, _rx, _shared) = app_with(FakeDiffSource::default());
    // A `last` count of zero cannot map to a NonZeroU32 target.
    let body = r#"{"cwd":"/x","store_root":"/y","target":{"kind":"last","count":0}}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/diffs/render")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    assert_eq!(json["notes"][0]["level"], "error");
}

#[tokio::test]
async fn render_against_a_non_repo_is_a_500_error_envelope() {
    // top_level: None ⇒ the diff source reports "not a git repository".
    let (app, _rx, _shared) = app_with(FakeDiffSource::default());
    let body = r#"{"cwd":"/x","store_root":"/y","target":{"kind":"unpushed"}}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/diffs/render")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "error");
    let notes = json["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["level"], "error");
}

#[tokio::test]
async fn render_happy_path_is_a_200_ok_envelope_with_artifact_and_notes() {
    let (app, _rx, _shared) = app_with(happy_source());
    let body = r#"{"cwd":"/repo","store_root":"/store","target":{"kind":"unpushed"}}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/diffs/render")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    assert!(
        !json["data"]["artifact"].as_str().unwrap().is_empty(),
        "artifact path must be non-empty"
    );
    let notes = json["notes"].as_array().unwrap();
    assert!(
        notes
            .iter()
            .any(|n| n["text"].as_str().unwrap().contains("diff-preview:")),
        "notes must carry the diff-preview summary line: {notes:?}"
    );
}

/// A scripted happy-path source for the merge-diff endpoint: unlike
/// [`happy_source`], `render_merge_diff` calls `verify_commit` against the
/// (default) base, so `known_revs` must list it.
fn merge_happy_source() -> FakeDiffSource {
    FakeDiffSource {
        top_level: Some("/repo".into()),
        branch: "feature".into(),
        known_revs: vec!["main".into()],
        commits: vec![domain::diffs::Commit {
            sha: "abc1234".into(),
            subject: "feat: work".into(),
            ..Default::default()
        }],
        diff_output: SINGLE_FILE_DIFF.into(),
        committed_at: "2026-07-02".into(),
        ..Default::default()
    }
}

async fn post(app: Router, uri: &str, body: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn merge_happy_path_is_a_200_ok_envelope_with_artifact() {
    let (app, _rx, _shared) = app_with(merge_happy_source());
    let body = r#"{"cwd":"/repo","store_root":"/store"}"#;

    let response = post(app, "/diffs/merge", body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    assert!(
        !json["data"]["artifact"].as_str().unwrap().is_empty(),
        "artifact path must be non-empty"
    );
}

#[tokio::test]
async fn squash_preview_happy_path_is_a_200_ok_envelope_with_artifact() {
    let (app, _rx, _shared) = app_with(happy_source());
    let body = r#"{"cwd":"/repo","store_root":"/store"}"#;

    let response = post(app, "/diffs/squash-preview", body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    assert!(
        !json["data"]["artifact"].as_str().unwrap().is_empty(),
        "artifact path must be non-empty"
    );
}

#[tokio::test]
async fn all_happy_path_is_a_200_ok_envelope_with_artifact() {
    let (app, _rx, _shared) = app_with(happy_source());
    let body = r#"{"store_root":"/store","root":"/scan-root","repos":[{"top":"/repo-a","label":"repo-a"}]}"#;

    let response = post(app, "/diffs/all", body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    assert!(
        !json["data"]["artifact"].as_str().unwrap().is_empty(),
        "artifact path must be non-empty"
    );
}

#[tokio::test]
async fn subrepos_happy_path_is_a_200_ok_envelope_with_artifact() {
    let (app, _rx, _shared) = app_with(happy_source());
    let body = r#"{"store_root":"/store","root":"/scan-root","target":{"kind":"unpushed"},"repos":[{"top":"/repo-a","label":"repo-a"}]}"#;

    let response = post(app, "/diffs/subrepos", body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    assert!(
        !json["data"]["artifact"].as_str().unwrap().is_empty(),
        "artifact path must be non-empty"
    );
}

#[tokio::test]
async fn subrepos_with_nothing_to_show_is_a_200_empty_envelope() {
    // Every field defaults empty: no commits, no diff — every repo's view is
    // empty, so the batch skips it and nothing gets rendered.
    let (app, _rx, _shared) = app_with(FakeDiffSource {
        upstream: Some("origin/main".into()),
        ..Default::default()
    });
    let body = r#"{"store_root":"/store","root":"/scan-root","target":{"kind":"unpushed"},"repos":[{"top":"/repo-a","label":"repo-a"}]}"#;

    let response = post(app, "/diffs/subrepos", body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "empty");
    assert!(json["data"].is_null());
}

#[tokio::test]
async fn managed_push_all_happy_path_is_a_200_ok_envelope_with_one_result() {
    let manifest = FakeManagedManifest {
        repos: vec![domain::managed::ManagedRepo {
            name: "repo".into(),
            path: "/repos/repo".into(),
            remote: String::new(),
        }],
        error: None,
    };
    let remote = FakeRemoteSync {
        present: false, // "skip" path — no real subprocess needed for a router test
        ..Default::default()
    };
    let (app, _rx, _shared) = app_with_managed(remote, manifest);
    let body = r#"{"repos_file":"/repos.toml","home_dir":"/home","dry":false}"#;

    let response = post(app, "/managed/push-all", body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    let results = json["data"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["Status"], "skip");
}

#[tokio::test]
async fn managed_pull_all_happy_path_is_a_200_ok_envelope_with_one_result() {
    let manifest = FakeManagedManifest {
        repos: vec![domain::managed::ManagedRepo {
            name: "repo".into(),
            path: "/repos/repo".into(),
            remote: String::new(),
        }],
        error: None,
    };
    let remote = FakeRemoteSync {
        present: false,
        ..Default::default()
    };
    let (app, _rx, _shared) = app_with_managed(remote, manifest);
    let body = r#"{"repos_file":"/repos.toml","home_dir":"/home","dry":false}"#;

    let response = post(app, "/managed/pull-all", body).await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    let results = json["data"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["Status"], "skip");
}
