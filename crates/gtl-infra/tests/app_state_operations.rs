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
        save_live_view::{self, SaveLiveView, SaveLiveViewOutcome},
    },
    ports::{Clock, GitRepositoryState},
    recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget},
    utils::FakeGitClient,
    viewer::{
        ViewerState,
        close_viewer_tabs::{self, CloseViewerTabs},
        work,
    },
};
use gtl_infra::app_state::SqliteAppState;
use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    recipes::RecipeBatchId,
    timestamps::MachineTimestamp,
    viewer::ViewerTabKind,
};
use rusqlite::Connection;

const CONCURRENT_SAVE_BUSY_RETRY_COUNT_MAX: i32 = 4_000;
const CONCURRENT_SAVE_BUSY_RETRY_DELAY: Duration = Duration::from_millis(1);
static CONCURRENT_SAVE_BUSY_SIGNAL_SENDER: Mutex<Option<mpsc::SyncSender<()>>> = Mutex::new(None);

fn concurrent_save_busy_signal_sender_set(sender: Option<mpsc::SyncSender<()>>) {
    *CONCURRENT_SAVE_BUSY_SIGNAL_SENDER.lock().unwrap() = sender;
}

fn concurrent_save_busy_handler(retry_count: i32) -> bool {
    if retry_count == 0
        && let Some(sender) = CONCURRENT_SAVE_BUSY_SIGNAL_SENDER.lock().unwrap().as_ref()
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
        Ok(MachineTimestamp::try_from("2026-07-19T00:00:00Z").unwrap())
    }
}

fn repository_root(path: &Path) -> RepositoryRoot {
    RepositoryRoot::try_new(path.to_path_buf()).unwrap()
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
    let mut connection = state.connection_lock().unwrap();
    let response = save_live_view::execute(
        SaveLiveView {
            comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
            path: top_level.to_path_buf(),
        },
        &git(top_level),
        &mut connection,
        &ClockTest,
    )
    .unwrap();
    assert!(matches!(
        response.outcome,
        SaveLiveViewOutcome::Created { .. }
    ));
}

fn list_live_views(state: &SqliteAppState) -> Vec<gtl_application::live_views::LiveViewRecord> {
    let connection = state.connection_lock().unwrap();
    list_live_views::execute(ListLiveViews, &connection).unwrap()
}

#[test]
fn public_operations_use_the_migrated_schema() {
    let directory = tempfile::tempdir().unwrap();
    let state = SqliteAppState::open(directory.path()).unwrap();
    let top_level = Path::new("/repos/alpha");

    save_live_view(&state, top_level);
    let live_views = list_live_views(&state);
    assert_eq!(live_views.len(), 1);
    assert_eq!(live_views[0].display_name.as_str(), "alpha");

    {
        let mut connection = state.connection_lock().unwrap();
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
        .unwrap();
    }
    let history = {
        let connection = state.connection_lock().unwrap();
        list_recent_render_page::execute(&ListRecentRenderPage::default(), &connection).unwrap()
    };
    assert_eq!(history.entries.len(), 1);
    let id = history.entries[0].id;
    let found = {
        let connection = state.connection_lock().unwrap();
        get_recent_render::execute(&GetRecentRender { id }, &connection).unwrap()
    };
    assert_eq!(found.unwrap().title, "alpha · unpushed");

    let viewer = ViewerState::new();
    let tab_id = work::reserve_open(
        &viewer,
        unpushed_diff_recipe(),
        RecipeBatchId::generate(),
        ViewerTabKind::Live,
    )
    .unwrap()
    .ticket()
    .tab_id;
    let refresh = {
        let mut connection = state.connection_lock().unwrap();
        close_viewer_tabs::execute(CloseViewerTabs::One(tab_id), &mut connection, &viewer).unwrap()
    };
    assert!(refresh.is_none());
    assert!(list_live_views(&state).is_empty());
}

#[test]
fn second_process_style_connection_observes_committed_rows() {
    let directory = tempfile::tempdir().unwrap();
    let state_first = SqliteAppState::open(directory.path()).unwrap();
    save_live_view(&state_first, Path::new("/repos/reopened"));

    let state_second = SqliteAppState::open(directory.path()).unwrap();

    assert_eq!(list_live_views(&state_second).len(), 1);
}

#[test]
fn concurrent_save_serializes_existence_check_and_upsert_across_connections() {
    concurrent_save_busy_signal_sender_set(None);
    let directory = tempfile::tempdir().unwrap();
    let state_first = SqliteAppState::open(directory.path()).unwrap();
    let state_second = SqliteAppState::open(directory.path()).unwrap();
    let reservation = Connection::open(directory.path().join("gtl.db")).unwrap();
    reservation.execute_batch("BEGIN IMMEDIATE").unwrap();

    for state in [&state_first, &state_second] {
        state
            .connection_lock()
            .unwrap()
            .busy_handler(Some(concurrent_save_busy_handler))
            .unwrap();
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
                let mut connection = state.connection_lock().unwrap();
                let result = save_live_view::execute(
                    SaveLiveView {
                        comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
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
    reservation_release_result.unwrap();
    assert_eq!(completion_receive_error, None);
    assert!(
        join_results.iter().all(Result::is_ok),
        "save workers must not panic"
    );

    let mut refresh_flags: Vec<_> = completion_results
        .into_iter()
        .map(|result| result.unwrap())
        .collect();
    refresh_flags.sort_unstable();
    assert_eq!(refresh_flags, vec![false, true]);
}

#[test]
fn prune_failure_rolls_back_the_render_insertion() {
    let directory = tempfile::tempdir().unwrap();
    let state = SqliteAppState::open(directory.path()).unwrap();
    state
        .connection_lock()
        .unwrap()
        .execute_batch(
            "INSERT INTO render_sources (id, kind, value, created_at)
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
        .unwrap();

    let result = {
        let mut connection = state.connection_lock().unwrap();
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
    let connection = state.connection_lock().unwrap();
    let render_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM recent_renders", [], |row| row.get(0))
        .unwrap();
    let failed_insertion_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM recent_renders WHERE title = 'failed insertion'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(render_count, 500);
    assert_eq!(failed_insertion_count, 0);
}

#[test]
fn project_comparisons_restore_independently_and_repeat_renders_update_recency()
-> anyhow::Result<()> {
    use gtl_application::{
        projects::open_viewer_project::{self, OpenProjectComparison},
        utils::{FixedClock, FixedUserSettingsStore},
    };
    use gtl_infra::git_client::HybridGitClient;
    use gtl_models::projects::ProjectRepository;
    use gtl_wire::viewer::projects::{OpenViewerProject, ViewerProjectDiffMode};

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("alpha");
    create_project_comparison_repository(&root);
    commit_project_comparison(&root);
    let path = repository_root(&root);
    let repositories = vec![ProjectRepository {
        name: ProjectName::try_new("Alpha").unwrap(),
        path: path.clone(),
        remote: None,
    }];
    let state = SqliteAppState::open(directory.path()).unwrap();
    let viewer = ViewerState::new();
    let mut connection = state.connection_lock().unwrap();
    let mut ids = Vec::new();
    for mode in [ViewerProjectDiffMode::Snapshot, ViewerProjectDiffMode::Live] {
        let request = OpenViewerProject {
            path: path.clone(),
            mode,
        };
        let pending = open_viewer_project::execute(
            OpenProjectComparison {
                project: request.clone(),
                repositories: repositories.clone(),
            },
            &HybridGitClient,
            &mut connection,
            &ClockTest,
            &viewer,
        )
        .unwrap();
        let id = pending.ticket().tab_id;
        ids.push(id);
        let computed = work::compute_recipe(
            pending,
            &FixedUserSettingsStore::default(),
            &HybridGitClient,
            &gtl_application::utils::ProjectComparisons::default(),
        );
        let work::RecipePublication::Published { history } =
            work::publish_recipe(&viewer, computed).unwrap()
        else {
            anyhow::bail!("valid comparison must publish");
        };
        record_render::execute(
            &history,
            &mut connection,
            &FixedClock::new("2026-09-06T10:00:00Z".try_into().unwrap()),
        )
        .unwrap();
        record_render::execute(
            &history,
            &mut connection,
            &FixedClock::new("2026-09-06T11:00:00Z".try_into().unwrap()),
        )
        .unwrap();
        let repeated = open_viewer_project::execute(
            OpenProjectComparison {
                project: request,
                repositories: repositories.clone(),
            },
            &HybridGitClient,
            &mut connection,
            &ClockTest,
            &viewer,
        )
        .unwrap();
        assert_eq!(repeated.ticket().tab_id, id);
    }
    assert_ne!(ids[0], ids[1]);
    assert_viewer_project_recency(directory.path(), &connection)?;
    assert_live_restores_independently(&mut connection, &viewer, ids[1]);
    Ok(())
}

fn assert_viewer_project_recency(
    home: &std::path::Path,
    connection: &rusqlite::Connection,
) -> anyhow::Result<()> {
    use gtl_application::projects::{get_viewer_project_status, list_viewer_projects};
    use gtl_infra::git_client::HybridGitClient;
    register_viewer_project(connection, "ALP", "Alpha", "~/alpha")?;
    let projects = list_viewer_projects::execute(
        &gtl_wire::viewer::projects::ListViewerProjects::default(),
        home,
        connection,
    )?;
    let project = &projects.projects()[0];
    let status = get_viewer_project_status::execute(project.clone(), &HybridGitClient)?;
    assert!(matches!(
        &status.status,
        gtl_models::repository::status::RepositoryStatus::Present {
            head: gtl_models::repository::status::StatusHead::Branch {
                upstream: gtl_models::repository::status::StatusUpstream::Tracking { .. },
                ..
            },
            ..
        }
    ));
    assert_eq!(
        project.last_rendered_at.as_ref().unwrap().as_ref(),
        "2026-09-06T11:00:00Z"
    );
    Ok(())
}

fn commit_project_comparison(root: &std::path::Path) {
    for args in [&["add", "."][..], &["commit", "-qm", "feature"][..]] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
}

fn create_project_comparison_repository(root: &std::path::Path) {
    std::fs::create_dir(root).unwrap();
    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run_git(&["init", "-qb", "main"]);
    run_git(&["config", "user.email", "test@example.test"]);
    run_git(&["config", "user.name", "Test"]);
    std::fs::write(root.join("file.txt"), "base\n").unwrap();
    run_git(&["add", "."]);
    run_git(&["commit", "-qm", "base"]);
    run_git(&["checkout", "-qb", "feature"]);
    run_git(&["branch", "--set-upstream-to=main"]);
    std::fs::write(root.join("new.txt"), "new\n").unwrap();
}

fn assert_live_restores_independently(
    connection: &mut rusqlite::Connection,
    viewer: &ViewerState,
    live_tab: gtl_models::viewer::ViewerTabId,
) {
    let saved = list_live_views::execute(ListLiveViews, connection).unwrap();
    assert_eq!(saved.len(), 1);
    let restored = ViewerState::new();
    work::reserve_restored_live_views(&restored, saved).unwrap();
    assert_eq!(restored.inspect(|session| session.tabs().len()).unwrap(), 1);
    close_viewer_tabs::execute(CloseViewerTabs::One(live_tab), connection, viewer).unwrap();
    assert!(
        list_live_views::execute(ListLiveViews, connection)
            .unwrap()
            .is_empty()
    );
}

fn register_viewer_project(
    connection: &rusqlite::Connection,
    id: &str,
    name: &str,
    source: &str,
) -> anyhow::Result<()> {
    connection.execute(
        "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', ?1)",
        [source],
    )?;
    connection.execute("INSERT INTO projects (id, title, source_id, mux_session_name, affiliation) VALUES (?1, ?2, last_insert_rowid(), ?1, 'personal')", rusqlite::params![id, name])?;
    Ok(())
}

#[test]
fn viewer_projects_page_stored_sources_without_git_or_status_ordering() -> anyhow::Result<()> {
    use gtl_application::projects::{
        list_viewer_projects,
        record_project_render::{self, RecordProjectRender},
    };
    use gtl_wire::viewer::projects::{
        ListViewerProjects, ViewerProjectsCursor, ViewerProjectsPageSize,
    };

    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let connection = state.connection_lock()?;
    for (id, name) in [("GAM", "Alpha"), ("BET", "Gamma"), ("ALP", "Beta")] {
        register_viewer_project(&connection, id, name, &format!("~/{id}"))?;
    }
    record_project_render::execute(
        &RecordProjectRender {
            path: repository_root(&directory.path().join("GAM")),
            rendered_at: "2026-09-06T13:00:00Z".try_into()?,
        },
        &connection,
    )?;
    let request = ListViewerProjects {
        sort: None,
        page_size: ViewerProjectsPageSize::try_new(2)?,
        cursor: ViewerProjectsCursor::First,
    };
    let page = list_viewer_projects::execute(&request, directory.path(), &connection)?;
    assert_eq!(
        page.projects()
            .iter()
            .map(|project| project.id.as_ref())
            .collect::<Vec<_>>(),
        ["ALP", "BET"]
    );
    assert_eq!(page.total(), 3);
    assert_eq!(page.count_before(), 0);
    let page = list_viewer_projects::execute(
        &ListViewerProjects {
            cursor: ViewerProjectsCursor::After("BET".try_into()?),
            ..request
        },
        directory.path(),
        &connection,
    )?;
    assert_eq!(page.projects()[0].id.as_ref(), "GAM");
    assert_eq!(
        page.projects()[0]
            .last_rendered_at
            .as_ref()
            .unwrap()
            .as_ref(),
        "2026-09-06T13:00:00Z"
    );
    assert_eq!(page.count_before(), 2);
    assert!(!page.projects()[0].path.as_ref().exists());
    Ok(())
}

#[test]
fn app_state_schema_matches_the_committed_snapshot() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let connection = database.connection_lock()?;
    let schema = connection.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter().map(|sql| format!("{sql};")).collect::<Vec<_>>().join("\n\n") + "\n";
    assert_eq!(schema, include_str!("../db/schema.sql"));
    Ok(())
}

#[test]
fn sqlite_sorts_every_change_combination_before_project_pagination() -> anyhow::Result<()> {
    use ProjectsSort::{Branch, BranchDescending, Changes, ChangesAscending, Name, NameDescending};
    use ViewerProjectsCursor::{After, Before, First, Last};
    use gtl_application::projects::list_viewer_projects;
    use gtl_models::settings::ProjectsSort;
    use gtl_wire::viewer::projects::{
        ListViewerProjects, ViewerProjectsCursor, ViewerProjectsPageSize,
    };
    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let connection = state.connection_lock()?;
    register_project_status_combinations(directory.path(), &connection)?;
    let assert_page = |sort, cursor, expected: &[&str], count_before| -> anyhow::Result<()> {
        let request = ListViewerProjects {
            cursor,
            page_size: ViewerProjectsPageSize::try_new(3)?,
            sort: Some(sort),
        };
        let page = list_viewer_projects::execute(&request, directory.path(), &connection)?;
        assert_eq!(
            page.projects()
                .iter()
                .map(|project| project.id.as_ref())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!((page.total(), page.count_before()), (10, count_before));
        Ok(())
    };
    assert_page(Changes, First, &["HH", "GG", "FF"], 0)?;
    assert_page(Changes, After("FF".try_into()?), &["II", "EE", "DD"], 3)?;
    assert_page(Changes, Before("II".try_into()?), &["HH", "GG", "FF"], 0)?;
    assert_page(Changes, Last, &["ZZ"], 9)?;
    assert_page(Changes, Before("ZZ".try_into()?), &["CC", "BB", "AA"], 6)?;
    assert_page(Name, First, &["II", "HH", "GG"], 0)?;
    assert_page(Branch, First, &["AA", "II", "HH"], 0)?;
    assert_page(ChangesAscending, First, &["AA", "BB", "CC"], 0)?;
    assert_page(
        ChangesAscending,
        After("CC".try_into()?),
        &["DD", "EE", "II"],
        3,
    )?;
    assert_page(
        ChangesAscending,
        Before("DD".try_into()?),
        &["AA", "BB", "CC"],
        0,
    )?;
    assert_page(ChangesAscending, Last, &["ZZ"], 9)?;
    assert_page(NameDescending, First, &["ZZ", "AA", "BB"], 0)?;
    assert_page(
        NameDescending,
        After("BB".try_into()?),
        &["CC", "DD", "EE"],
        3,
    )?;
    assert_page(BranchDescending, First, &["II", "HH", "GG"], 0)?;
    assert_page(BranchDescending, Last, &["ZZ"], 9)?;
    let candidates =
        list_viewer_projects::status_refresh_candidates(directory.path(), &connection)?;
    assert_eq!(
        candidates
            .iter()
            .map(|project| project.id.as_ref())
            .collect::<Vec<_>>(),
        ["ZZ"]
    );
    Ok(())
}

fn register_project_status_combinations(
    home: &std::path::Path,
    connection: &rusqlite::Connection,
) -> anyhow::Result<()> {
    use gtl_application::projects::{list_viewer_projects, status_index};
    use gtl_models::repository::{
        PathCount,
        status::{RepositoryStatus, StatusChanges, StatusHead, StatusUpstream},
    };
    use gtl_wire::viewer::projects::{ViewerProjectBranchComparison, ViewerProjectStatus};
    let cases = [
        ("AA", 0, 0, 0),
        ("BB", 0, 0, 1),
        ("CC", 0, 1, 0),
        ("DD", 0, 1, 1),
        ("EE", 2, 0, 0),
        ("FF", 1, 0, 1),
        ("GG", 1, 1, 0),
        ("HH", 1, 1, 1),
        ("II", 10, 0, 0),
    ];
    for (index, (id, ahead, tracked, untracked)) in cases.into_iter().enumerate() {
        register_viewer_project(
            connection,
            id,
            &format!("Project {:02}", 20 - index),
            &format!("~/{id}"),
        )?;
        let project =
            list_viewer_projects::get_project(&id.try_into()?, home, connection)?.unwrap();
        let status = ViewerProjectStatus {
            project_id: project.id.clone(),
            comparison_branch: project.comparison_branch.clone(),
            status: RepositoryStatus::Present {
                head: StatusHead::Branch {
                    name: if id == "AA" { "alpha" } else { "zulu" }.try_into()?,
                    upstream: StatusUpstream::Missing,
                },
                changes: StatusChanges::from_counts(
                    PathCount::new(tracked),
                    PathCount::new(untracked),
                ),
            },
            branch_comparison: ViewerProjectBranchComparison::Branch {
                commits_ahead: gtl_models::git::CommitCount::new(ahead),
            },
        };
        status_index::record(&project, Some(&status), home, connection)?;
        status_index::record(&project, None, home, connection)?;
    }
    register_viewer_project(connection, "ZZ", "Unavailable", "~/ZZ")?;
    Ok(())
}

#[test]
fn project_status_index_ignores_results_for_replaced_repositories() -> anyhow::Result<()> {
    use gtl_application::projects::{list_viewer_projects, status_index};

    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let connection = database.connection_lock()?;
    register_viewer_project(&connection, "AA", "Original", "~/original")?;
    connection.execute(
        "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', '~/replacement')",
        [],
    )?;
    let replacement = connection.last_insert_rowid();
    let original =
        list_viewer_projects::get_project(&"AA".try_into()?, directory.path(), &connection)?
            .unwrap();
    connection.execute(
        "UPDATE projects SET source_id = ?1 WHERE id = 'AA'",
        [replacement],
    )?;
    status_index::record(&original, None, directory.path(), &connection)?;
    let indexed: u32 =
        connection.query_row("SELECT count(*) FROM project_status_index", [], |row| {
            row.get(0)
        })?;
    assert_eq!(indexed, 0);
    let current =
        list_viewer_projects::get_project(&"AA".try_into()?, directory.path(), &connection)?
            .unwrap();
    status_index::record(&current, None, directory.path(), &connection)?;
    let indexed: u32 =
        connection.query_row("SELECT count(*) FROM project_status_index", [], |row| {
            row.get(0)
        })?;
    assert_eq!(indexed, 1);
    Ok(())
}
