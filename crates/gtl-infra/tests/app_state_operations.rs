#![cfg(test)]

use std::{
    path::Path,
    sync::{Arc, Barrier, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use gtl_application::{
    history::{
        get_recent_render::{self, GetRecentRender},
        list_recent_render_page::{self, ListRecentRenderPage},
        record_render::{self, RecordRender, RecordRenderError},
    },
    live_views::{
        list_live_views::{self, ListLiveViews},
        remove_live_view::{self, RemoveLiveView},
        save_live_view::{self, SaveLiveView, SaveLiveViewOutcome},
    },
    ports::{Clock, GitRepositoryState},
    utils::FakeGitClient,
};
use gtl_infra::app_state::SqliteAppState;
use gtl_models::{
    live_views::LiveSource,
    paths::{ProjectName, RepositoryRoot},
    timestamps::MachineTimestamp,
};
use gtl_wire::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};
use rusqlite::Connection;

const CONCURRENT_SAVE_BUSY_RETRY_COUNT_MAX: i32 = 4_000;
const CONCURRENT_SAVE_BUSY_RETRY_DELAY: Duration = Duration::from_millis(1);
static CONCURRENT_SAVE_BUSY_SIGNAL_SENDER: Mutex<Option<mpsc::SyncSender<()>>> = Mutex::new(None);

fn concurrent_save_busy_signal_sender_set(sender: Option<mpsc::SyncSender<()>>) {
    *CONCURRENT_SAVE_BUSY_SIGNAL_SENDER
        .lock()
        .expect("lock concurrent-save busy-signal sender") = sender;
}

fn concurrent_save_busy_handler(retry_count: i32) -> bool {
    if retry_count == 0
        && let Some(sender) = CONCURRENT_SAVE_BUSY_SIGNAL_SENDER
            .lock()
            .expect("lock concurrent-save busy-signal sender")
            .as_ref()
    {
        let _ = sender.try_send(());
    }
    thread::sleep(CONCURRENT_SAVE_BUSY_RETRY_DELAY);
    retry_count < CONCURRENT_SAVE_BUSY_RETRY_COUNT_MAX
}

#[derive(Clone)]
struct ClockTest;

impl Clock for ClockTest {
    fn now(&self) -> Result<MachineTimestamp, gtl_models::timestamps::TimestampError> {
        Ok(MachineTimestamp::try_from("2026-07-19T00:00:00Z")
            .expect("fixture clock timestamp is valid"))
    }
}

fn repository_root(path: &Path) -> RepositoryRoot {
    RepositoryRoot::try_new(path.to_path_buf()).expect("fixture repository root is absolute")
}

fn git(top_level: &Path) -> FakeGitClient {
    FakeGitClient {
        repository_state: Some(GitRepositoryState::Repository {
            top_level: repository_root(top_level),
        }),
        ..Default::default()
    }
}

fn unpushed_diff_recipe() -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(repository_root(Path::new("/repos/alpha"))),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: None,
    }
}

fn save_live_view(state: &SqliteAppState, top_level: &Path) {
    let mut connection = state.connection_lock().expect("lock state connection");
    let response = save_live_view::execute(
        SaveLiveView {
            path: top_level.to_path_buf(),
        },
        &git(top_level),
        &mut connection,
        &ClockTest,
    )
    .expect("save live view");
    assert!(matches!(
        response.outcome,
        SaveLiveViewOutcome::Created { .. }
    ));
}

fn list_live_views(state: &SqliteAppState) -> Vec<gtl_application::live_views::LiveViewRecord> {
    let connection = state.connection_lock().expect("lock state connection");
    list_live_views::execute(ListLiveViews, &connection)
        .expect("list live views")
        .views
}

#[test]
fn public_operations_use_the_migrated_schema() {
    let directory = tempfile::tempdir().expect("temporary data root");
    let state = SqliteAppState::open(directory.path()).expect("open app state");
    let top_level = Path::new("/repos/alpha");

    save_live_view(&state, top_level);
    let live_views = list_live_views(&state);
    assert_eq!(live_views.len(), 1);
    assert_eq!(live_views[0].display_name.as_str(), "alpha");

    {
        let mut connection = state.connection_lock().expect("lock state connection");
        record_render::execute(
            &RecordRender {
                recipe: unpushed_diff_recipe(),
                title: "alpha · unpushed".into(),
                repo_name: ProjectName::try_from("alpha").unwrap(),
                range_label: "origin/main..HEAD".into(),
            },
            &mut connection,
            &ClockTest,
        )
        .expect("record render");
    }
    let history = {
        let connection = state.connection_lock().expect("lock state connection");
        list_recent_render_page::execute(ListRecentRenderPage::default(), &connection)
            .expect("list recent renders")
    };
    assert_eq!(history.entries.len(), 1);
    let id = history.entries[0].id;
    let found = {
        let connection = state.connection_lock().expect("lock state connection");
        get_recent_render::execute(&GetRecentRender { id }, &connection).expect("get recent render")
    };
    assert_eq!(
        found.entry.expect("recent render").title,
        "alpha · unpushed"
    );

    let removed = {
        let connection = state.connection_lock().expect("lock state connection");
        remove_live_view::execute(
            RemoveLiveView {
                source: LiveSource::local_repo(repository_root(top_level)),
            },
            &connection,
        )
        .expect("remove live view")
    };
    assert_eq!(removed, remove_live_view::RemoveLiveViewOk::Removed);
    assert!(list_live_views(&state).is_empty());
}

#[test]
fn second_process_style_connection_observes_committed_rows() {
    let directory = tempfile::tempdir().expect("temporary data root");
    let state_first = SqliteAppState::open(directory.path()).expect("open first app state");
    save_live_view(&state_first, Path::new("/repos/reopened"));

    let state_second = SqliteAppState::open(directory.path()).expect("open second app state");

    assert_eq!(list_live_views(&state_second).len(), 1);
}

#[test]
fn concurrent_save_serializes_existence_check_and_upsert_across_connections() {
    concurrent_save_busy_signal_sender_set(None);
    let directory = tempfile::tempdir().expect("temporary data root");
    let state_first = SqliteAppState::open(directory.path()).expect("open first app state");
    let state_second = SqliteAppState::open(directory.path()).expect("open second app state");
    let reservation =
        Connection::open(directory.path().join("gtl.db")).expect("open reservation connection");
    reservation
        .execute_batch("BEGIN IMMEDIATE")
        .expect("hold writer reservation");

    for state in [&state_first, &state_second] {
        state
            .connection_lock()
            .expect("lock state connection")
            .busy_handler(Some(concurrent_save_busy_handler))
            .expect("install busy handler");
    }

    let (busy_signal_sender, busy_signal_receiver) = mpsc::sync_channel(2);
    concurrent_save_busy_signal_sender_set(Some(busy_signal_sender));
    let barrier = Arc::new(Barrier::new(2));
    let (completion_sender, completion_receiver) = mpsc::sync_channel(2);
    let handles: Vec<_> = [state_first, state_second]
        .into_iter()
        .map(|state| {
            let barrier = Arc::clone(&barrier);
            let completion_sender = completion_sender.clone();
            thread::spawn(move || {
                barrier.wait();
                let mut connection = state.connection_lock().expect("lock state connection");
                let result = save_live_view::execute(
                    SaveLiveView {
                        path: "/repos/concurrent".into(),
                    },
                    &git(Path::new("/repos/concurrent")),
                    &mut connection,
                    &ClockTest,
                )
                .map_err(|error| error.to_string())
                .and_then(|response| match response.outcome {
                    SaveLiveViewOutcome::Created { .. } => Ok(false),
                    SaveLiveViewOutcome::Refreshed { .. } => Ok(true),
                    SaveLiveViewOutcome::Rejected { rejection } => Err(rejection.to_string()),
                });
                let _ = completion_sender.send(result);
            })
        })
        .collect();
    drop(completion_sender);

    let busy_signal_deadline = Instant::now() + Duration::from_secs(2);
    let mut busy_signal_results = Vec::with_capacity(2);
    for _ in 0..2 {
        busy_signal_results.push(
            busy_signal_receiver
                .recv_timeout(busy_signal_deadline.saturating_duration_since(Instant::now())),
        );
    }
    let completion_pending = completion_receiver.recv_timeout(Duration::from_millis(100));

    concurrent_save_busy_signal_sender_set(None);
    let reservation_release_result = reservation.execute_batch("ROLLBACK");

    let mut completion_results = Vec::with_capacity(2);
    if let Ok(result) = completion_pending.as_ref() {
        completion_results.push(result.clone());
    }
    let completion_deadline = Instant::now() + Duration::from_millis(4_500);
    let mut completion_receive_error = None;
    while completion_results.len() < 2 {
        match completion_receiver
            .recv_timeout(completion_deadline.saturating_duration_since(Instant::now()))
        {
            Ok(result) => completion_results.push(result),
            Err(error) => {
                completion_receive_error = Some(error);
                break;
            }
        }
    }
    let join_results: Vec<_> = handles.into_iter().map(thread::JoinHandle::join).collect();

    assert!(
        busy_signal_results.iter().all(Result::is_ok),
        "both connections must reach SQLite write contention: {busy_signal_results:?}"
    );
    assert!(
        matches!(completion_pending, Err(mpsc::RecvTimeoutError::Timeout)),
        "both saves must remain pending while the reservation is held"
    );
    reservation_release_result.expect("release writer reservation");
    assert_eq!(completion_receive_error, None);
    assert!(
        join_results.iter().all(Result::is_ok),
        "save workers must not panic"
    );

    let mut refresh_flags: Vec<_> = completion_results
        .into_iter()
        .map(|result| result.expect("save succeeds"))
        .collect();
    refresh_flags.sort_unstable();
    assert_eq!(refresh_flags, vec![false, true]);
}

#[test]
fn prune_failure_rolls_back_the_render_insertion() {
    let directory = tempfile::tempdir().expect("temporary data root");
    let state = SqliteAppState::open(directory.path()).expect("open app state");
    state
        .connection_lock()
        .expect("lock fixture connection")
        .execute_batch(
            "INSERT INTO project_sources (id, kind, value, created_at)
             VALUES (1, 'directory', '/repos/fixture', '2026-07-18T00:00:00Z');
             WITH RECURSIVE render_number(value) AS (
               SELECT 1
               UNION ALL
               SELECT value + 1 FROM render_number WHERE value < 500
             )
             INSERT INTO recent_renders
               (source_id, operation_id, target_id, pinned_base, pinned_head,
                title, repo_name, range_label, rendered_at)
             SELECT 1, 1, 1, 'base-' || value, 'head-' || value,
                    'seed ' || value, 'fixture', 'main..HEAD', '2026-07-18T00:00:00Z'
             FROM render_number;
             CREATE TRIGGER recent_renders_prune_abort
             BEFORE DELETE ON recent_renders
             BEGIN
               SELECT RAISE(ABORT, 'pruning blocked by test');
             END;",
        )
        .expect("seed capped history and install prune trigger");

    let result = {
        let mut connection = state.connection_lock().expect("lock record connection");
        record_render::execute(
            &RecordRender {
                recipe: unpushed_diff_recipe(),
                title: "failed insertion".into(),
                repo_name: ProjectName::try_from("alpha").unwrap(),
                range_label: "origin/main..HEAD".into(),
            },
            &mut connection,
            &ClockTest,
        )
    };

    assert!(matches!(result, Err(RecordRenderError::Unexpected(_))));
    let connection = state.connection_lock().expect("lock assertion connection");
    let render_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM recent_renders", [], |row| row.get(0))
        .expect("count recent renders");
    let failed_insertion_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM recent_renders WHERE title = 'failed insertion'",
            [],
            |row| row.get(0),
        )
        .expect("count failed insertion");

    assert_eq!(render_count, 500);
    assert_eq!(failed_insertion_count, 0);
}
