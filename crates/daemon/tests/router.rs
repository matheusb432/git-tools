//! Router-level tests exercising the concrete daemon state through real temporary boundaries.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use application::ports::AppStateStore;
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

static DATABASE_BUSY_SIGNAL_SENDER: std::sync::OnceLock<
    std::sync::Mutex<Option<std::sync::mpsc::SyncSender<()>>>,
> = std::sync::OnceLock::new();

fn database_busy_signal_sender_set(sender: Option<std::sync::mpsc::SyncSender<()>>) {
    *DATABASE_BUSY_SIGNAL_SENDER
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .expect("database busy signal lock") = sender;
}

fn database_busy_signal(_retry_count: i32) -> bool {
    if let Some(sender) = DATABASE_BUSY_SIGNAL_SENDER
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .expect("database busy signal lock")
        .as_ref()
    {
        let _ = sender.try_send(());
    }
    true
}

struct DatabaseBusySignalGuard<'a> {
    app_state: &'a infra::app_state::SqliteAppState,
}

impl<'a> DatabaseBusySignalGuard<'a> {
    fn install(
        app_state: &'a infra::app_state::SqliteAppState,
        sender: std::sync::mpsc::SyncSender<()>,
    ) -> Self {
        database_busy_signal_sender_set(Some(sender));
        app_state
            .connection_lock()
            .expect("app-state connection lock")
            .busy_handler(Some(database_busy_signal))
            .expect("install database busy signal");
        Self { app_state }
    }
}

impl Drop for DatabaseBusySignalGuard<'_> {
    fn drop(&mut self) {
        database_busy_signal_sender_set(None);
        if let Ok(connection) = self.app_state.connection_lock() {
            let _ = connection.busy_handler(None);
        }
    }
}

struct Fixture {
    _temporary: TempDir,
    app: Router,
    app_state: infra::app_state::SqliteAppState,
    shutdown_receiver: watch::Receiver<bool>,
    repository: PathBuf,
    data_root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().expect("temp dir");
        let repository = temporary.path().join("repo");
        let data_root = temporary.path().join("data");
        std::fs::create_dir_all(&repository).expect("repo dir");
        init_repo(&repository);

        let (shutdown_sender, shutdown_receiver) = watch::channel(false);
        let app_state = infra::app_state::SqliteAppState::open(&data_root).expect("open app state");
        let app = daemon::state::router(DaemonState::new(
            ExeIdentity {
                exe_len: 4242,
                exe_modified_ms: 111,
            },
            "9.9.9",
            4242,
            shutdown_sender,
            app_state.clone(),
            infra::user_config::TomlSettingsStore::new(None),
        ));
        Self {
            _temporary: temporary,
            app,
            app_state,
            shutdown_receiver,
            repository,
            data_root,
        }
    }

    async fn post(&self, uri: &str, body: Value) -> axum::response::Response {
        post(self.app.clone(), uri, &body.to_string()).await
    }
}

fn init_repo(repository: &Path) {
    git(repository, &["init", "-q", "-b", "main"]);
    git(repository, &["config", "user.email", "test@example.com"]);
    git(repository, &["config", "user.name", "Test User"]);
    std::fs::write(repository.join("f.txt"), "base\n").expect("base file");
    git(repository, &["add", "f.txt"]);
    git(repository, &["commit", "-qm", "initial"]);
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
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

#[tokio::test]
async fn health_returns_startup_identity() {
    let fixture = Fixture::new();
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
    let mut fixture = Fixture::new();
    assert!(!*fixture.shutdown_receiver.borrow());

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
        .shutdown_receiver
        .changed()
        .await
        .expect("shutdown change");
    assert!(*fixture.shutdown_receiver.borrow());
}

#[tokio::test]
async fn malformed_render_json_is_a_400_error_envelope() {
    let fixture = Fixture::new();
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
async fn live_view_save_ignores_legacy_data_root_and_uses_daemon_store() {
    let fixture = Fixture::new();
    let redirected_root = fixture.data_root.with_file_name("redirected");
    let response = fixture
        .post(
            "/live-views/save",
            json!({"data_root": redirected_root, "path": fixture.repository}),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["outcome"], "ok");
    assert_eq!(json["data"]["source_kind"], "LocalRepo");
    assert_eq!(json["data"]["display_name"], "repo");
    assert_eq!(json["data"]["already_saved"], false);

    let connection = rusqlite::Connection::open(fixture.data_root.join("gtl.db"))
        .expect("open daemon app database");
    let row_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM live_views \
             WHERE source_kind = 'LocalRepo' AND source_value = ?1",
            [fixture.repository.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .expect("count saved live view");
    assert_eq!(row_count, 1);
    assert!(!redirected_root.join("gtl.db").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_remains_responsive_while_live_view_save_waits_on_the_database() {
    let fixture = Fixture::new();
    let warm_response = fixture
        .post("/live-views/save", json!({"path": fixture.repository}))
        .await;
    assert_eq!(warm_response.status(), StatusCode::OK);

    let database_connection =
        rusqlite::Connection::open(fixture.data_root.join("gtl.db")).expect("open app database");
    database_connection
        .execute_batch("BEGIN IMMEDIATE")
        .expect("hold app database write lock");

    let (busy_signal_sender, busy_signal_receiver) = std::sync::mpsc::sync_channel(1);
    let database_busy_signal_guard =
        DatabaseBusySignalGuard::install(&fixture.app_state, busy_signal_sender);
    let save_router = fixture.app.clone();
    let save_body = json!({"path": fixture.repository}).to_string();
    let save_task =
        tokio::spawn(async move { post(save_router, "/live-views/save", &save_body).await });
    busy_signal_receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("save must reach SQLite contention");
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
    drop(database_busy_signal_guard);
}
