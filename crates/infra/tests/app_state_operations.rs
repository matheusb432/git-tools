use std::path::{Path, PathBuf};

use application::{
    history::{
        list_recent::{self, GetRecentRender, ListRecentRenders},
        record_render::{self, RecordRender},
    },
    live_views::{
        list::{self, ListLiveViews},
        remove::{self, RemoveLiveView},
        save::{self, SaveLiveView, SaveLiveViewOutcome},
    },
    ports::{Clock, RepoProbe, RepoProbeResult},
};
use infra::app_state::SqliteAppState;

#[derive(Clone)]
struct ClockTest;

impl Clock for ClockTest {
    fn now_iso(&self) -> String {
        "2026-07-19T00:00:00Z".into()
    }
}

#[derive(Clone)]
struct RepoProbeTest {
    top_level: PathBuf,
}

impl RepoProbe for RepoProbeTest {
    fn probe(&self, _directory: &Path) -> anyhow::Result<RepoProbeResult> {
        Ok(RepoProbeResult::Repo {
            top_level: self.top_level.clone(),
        })
    }
}

fn save_live_view(state: &SqliteAppState, data_root: &Path, top_level: &Path) {
    let response = save::execute(
        SaveLiveView {
            data_root: data_root.to_path_buf(),
            path: top_level.to_path_buf(),
        },
        &RepoProbeTest {
            top_level: top_level.to_path_buf(),
        },
        state,
        &ClockTest,
    )
    .expect("save live view");
    assert!(matches!(
        response.outcome,
        SaveLiveViewOutcome::Saved {
            already_saved: false,
            ..
        }
    ));
}

fn list_live_views(
    state: &SqliteAppState,
    data_root: &Path,
) -> Vec<application::live_views::LiveViewRecord> {
    list::execute(
        ListLiveViews {
            data_root: data_root.to_path_buf(),
        },
        state,
    )
    .expect("list live views")
    .views
}

#[test]
fn public_operations_use_the_migrated_schema() {
    let directory = tempfile::tempdir().expect("temporary data root");
    let state = SqliteAppState::open(directory.path()).expect("open app state");
    let top_level = Path::new("/repos/alpha");

    save_live_view(&state, directory.path(), top_level);
    let live_views = list_live_views(&state, directory.path());
    assert_eq!(live_views.len(), 1);
    assert_eq!(live_views[0].display_name, "alpha");

    record_render::execute(
        RecordRender {
            data_root: directory.path().to_path_buf(),
            recipe_json: r#"{"kind":"diff"}"#.into(),
            title: "alpha · unpushed".into(),
            repo_name: "alpha".into(),
            kind: "diff".into(),
            range_label: "origin/main..HEAD".into(),
        },
        &state,
        &ClockTest,
    )
    .expect("record render");
    let history = list_recent::list::execute(
        ListRecentRenders {
            data_root: directory.path().to_path_buf(),
        },
        &state,
    )
    .expect("list recent renders");
    assert_eq!(history.entries.len(), 1);
    let id = history.entries[0].id;
    let found = list_recent::get::execute(
        GetRecentRender {
            data_root: directory.path().to_path_buf(),
            id,
        },
        &state,
    )
    .expect("get recent render");
    assert_eq!(
        found.entry.expect("recent render").title,
        "alpha · unpushed"
    );

    let removed = remove::execute(
        RemoveLiveView {
            data_root: directory.path().to_path_buf(),
            source_kind: "LocalRepo".into(),
            source_value: top_level.display().to_string(),
        },
        &state,
    )
    .expect("remove live view");
    assert!(removed.removed);
    assert!(list_live_views(&state, directory.path()).is_empty());
}

#[test]
fn clones_observe_the_same_persisted_rows() {
    let directory = tempfile::tempdir().expect("temporary data root");
    let state = SqliteAppState::open(directory.path()).expect("open app state");
    let state_clone = state.clone();

    save_live_view(&state, directory.path(), Path::new("/repos/clone"));

    assert_eq!(list_live_views(&state_clone, directory.path()).len(), 1);
}

#[test]
fn second_process_style_connection_observes_committed_rows() {
    let directory = tempfile::tempdir().expect("temporary data root");
    let state_first = SqliteAppState::open(directory.path()).expect("open first app state");
    save_live_view(&state_first, directory.path(), Path::new("/repos/reopened"));

    let state_second = SqliteAppState::open(directory.path()).expect("open second app state");

    assert_eq!(list_live_views(&state_second, directory.path()).len(), 1);
}
