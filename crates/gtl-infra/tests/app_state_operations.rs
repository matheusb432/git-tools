#![cfg(test)]

use std::path::{Path, PathBuf};

use gtl_application::{
    history::{
        get_recent_render::{self, GetRecentRender},
        list_recent_render_page::{self, ListRecentRenderPage},
        record_render::{self, RecordRender, RecordRenderError},
    },
    ports::Clock,
    recipes::{Recipe, RecipeLabelParts, RecipeOp, RecipeSource, RecipeTarget},
    viewer::{
        ViewerState,
        saved_tabs::{self, SavedViewerTab},
        work,
    },
};
use gtl_infra::{app_state::SqliteAppState, testing::TestRepository};
use gtl_models::{
    git::CommitCount,
    paths::{ProjectName, RepositoryRoot},
    timestamps::MachineTimestamp,
};

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

fn unpushed_diff_recipe() -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(repository_root(Path::new(
            "//fixture.invalid/repositories/repos/alpha",
        ))),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: None,
    }
}

fn saved_tab(recipe: Recipe) -> SavedViewerTab {
    SavedViewerTab {
        history_id: None,
        comparison_name: None,
        label: gtl_models::recipes::RecipeLabel::Repository {
            repository: recipe.cwd().project_name(),
        },
        recipe,
        pinned: false,
        live: true,
        active: true,
    }
}

fn save_tabs(state: &SqliteAppState, tabs: &[SavedViewerTab]) {
    saved_tabs::save(&mut state.connection_lock().unwrap(), tabs).unwrap();
}

fn load_tabs(state: &SqliteAppState) -> Vec<SavedViewerTab> {
    saved_tabs::load(&state.connection_lock().unwrap()).unwrap()
}

#[test]
fn public_operations_use_the_migrated_schema() {
    let directory = tempfile::tempdir().unwrap();
    let state = SqliteAppState::open(directory.path()).unwrap();

    let tabs = [saved_tab(unpushed_diff_recipe())];
    save_tabs(&state, &tabs);
    assert_eq!(load_tabs(&state), tabs);

    {
        let mut connection = state.connection_lock().unwrap();
        record_render::execute(
            &RecordRender {
                comparison_name: None,
                recipe: unpushed_diff_recipe(),
                repo_name: ProjectName::try_from("alpha").unwrap(),
                range_label: "origin/main..HEAD".into(),
                label_parts: RecipeLabelParts::UnpushedCommits {
                    count: CommitCount::new(2),
                },
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
    assert_eq!(
        found.unwrap().label_parts,
        RecipeLabelParts::UnpushedCommits {
            count: CommitCount::new(2),
        }
    );
}

#[test]
fn second_process_style_connection_observes_committed_rows() {
    let directory = tempfile::tempdir().unwrap();
    let state_first = SqliteAppState::open(directory.path()).unwrap();
    let tabs = [saved_tab(unpushed_diff_recipe())];
    save_tabs(&state_first, &tabs);

    let state_second = SqliteAppState::open(directory.path()).unwrap();

    assert_eq!(load_tabs(&state_second), tabs);
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
             VALUES (1, 'directory', '//fixture.invalid/repositories/repos/fixture', '2026-07-18T00:00:00Z');
             WITH RECURSIVE render_number(value) AS (
               SELECT 1
               UNION ALL
               SELECT value + 1 FROM render_number WHERE value < 500
             )
             INSERT INTO recent_renders
               (source_id, operation_id, target_id, pinned_base, pinned_head,
                repo_name, range_label, rendered_at)
             SELECT 1, 1, 1, 'base-' || value, 'head-' || value,
                    'fixture', 'main..HEAD', '2026-07-18T00:00:00Z'
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
                comparison_name: None,
                recipe: unpushed_diff_recipe(),
                repo_name: ProjectName::try_from("alpha").unwrap(),
                range_label: "origin/main..HEAD".into(),
                label_parts: RecipeLabelParts::None,
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
            "SELECT COUNT(*) FROM recent_renders WHERE repo_name = 'alpha'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(render_count, 500);
    assert_eq!(failed_insertion_count, 0);
}

#[test]
fn project_comparisons_reopen_their_tab() -> anyhow::Result<()> {
    use gtl_application::{
        projects::open_viewer_project::{self, OpenProjectComparison},
        utils::{FixedClock, FixedUserSettingsStore},
    };
    use gtl_infra::git_client::HybridGitClient;
    use gtl_models::projects::ProjectRepository;
    use gtl_wire::viewer::projects::OpenViewerProject;

    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().canonicalize().unwrap();
    let repository = project_comparison_repository(home.join("alpha"));
    let path = repository.root();
    let repositories = vec![ProjectRepository {
        name: ProjectName::try_new("Alpha").unwrap(),
        path: path.clone(),
        remote: None,
    }];
    let state = SqliteAppState::open(directory.path()).unwrap();
    let viewer = ViewerState::new();
    let mut connection = state.connection_lock().unwrap();
    let request = OpenViewerProject { path };
    let pending = open_viewer_project::execute(
        OpenProjectComparison {
            project: request.clone(),
            repositories: repositories.clone(),
        },
        &HybridGitClient,
        &viewer,
    )
    .unwrap();
    let id = pending.ticket().tab_id;
    let computed = work::compute_recipe(
        pending,
        &FixedUserSettingsStore::default(),
        &HybridGitClient,
        &gtl_application::utils::SavedExtensionFilters::default(),
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
    let repeated = open_viewer_project::execute(
        OpenProjectComparison {
            project: request,
            repositories,
        },
        &HybridGitClient,
        &viewer,
    )
    .unwrap();
    assert_eq!(repeated.ticket().tab_id, id);
    assert_viewer_project_status(&home, &connection)?;
    Ok(())
}

fn assert_viewer_project_status(
    home: &std::path::Path,
    connection: &rusqlite::Connection,
) -> anyhow::Result<()> {
    use gtl_application::projects::{get_viewer_project_status, list_viewer_projects};
    use gtl_infra::git_client::HybridGitClient;
    register_viewer_project(
        connection,
        "ALP",
        "Alpha",
        home.join("alpha").to_str().unwrap(),
    )?;
    let projects = list_viewer_projects::execute(
        &gtl_wire::viewer::projects::ListViewerProjects::default(),
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
    Ok(())
}

/// Initializes a `feature` branch one commit ahead of the local `main` it tracks.
fn project_comparison_repository(path: PathBuf) -> TestRepository {
    let repository = TestRepository::init(path);
    repository.write("file.txt", "base\n");
    repository.commit_all("base");
    repository.git(&["checkout", "-qb", "feature"]);
    repository.git(&["branch", "--set-upstream-to=main"]);
    repository.write("new.txt", "new\n");
    repository.commit_all("feature");
    repository
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
    connection.execute(
        "INSERT INTO projects (id, title, source_id) VALUES (?1, ?2, last_insert_rowid())",
        rusqlite::params![id, name],
    )?;
    Ok(())
}

#[test]
fn viewer_projects_page_stored_sources_without_git_or_status_ordering() -> anyhow::Result<()> {
    use gtl_application::projects::list_viewer_projects;
    use gtl_wire::viewer::projects::{
        ListViewerProjects, ViewerProjectsCursor, ViewerProjectsPageSize,
    };

    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let connection = state.connection_lock()?;
    for (id, name) in [("GAM", "Alpha"), ("BET", "Gamma"), ("ALP", "Beta")] {
        register_viewer_project(
            &connection,
            id,
            name,
            directory.path().join(id).to_str().unwrap(),
        )?;
    }
    let request = ListViewerProjects {
        sort: None,
        page_size: ViewerProjectsPageSize::try_new(2)?,
        cursor: ViewerProjectsCursor::First,
    };
    let page = list_viewer_projects::execute(&request, &connection)?;
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
        &connection,
    )?;
    assert_eq!(page.projects()[0].id.as_ref(), "GAM");
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
    register_project_status_combinations(&connection)?;
    let assert_page = |sort, cursor, expected: &[&str], count_before| -> anyhow::Result<()> {
        let request = ListViewerProjects {
            cursor,
            page_size: ViewerProjectsPageSize::try_new(3)?,
            sort: Some(sort),
        };
        let page = list_viewer_projects::execute(&request, &connection)?;
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
    let candidates = list_viewer_projects::status_refresh_candidates(&connection)?;
    assert_eq!(
        candidates
            .iter()
            .map(|project| project.id.as_ref())
            .collect::<Vec<_>>(),
        ["ZZ"]
    );
    Ok(())
}

fn register_project_status_combinations(connection: &rusqlite::Connection) -> anyhow::Result<()> {
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
            &format!("//fixture.invalid/repositories/repos/{id}"),
        )?;
        let project = list_viewer_projects::get_project(&id.try_into()?, connection)?.unwrap();
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
        status_index::record(&project, Some(&status), connection)?;
        status_index::record(&project, None, connection)?;
    }
    register_viewer_project(
        connection,
        "ZZ",
        "Unavailable",
        "//fixture.invalid/repositories/repos/ZZ",
    )?;
    Ok(())
}

#[test]
fn project_status_index_ignores_results_for_replaced_repositories() -> anyhow::Result<()> {
    use gtl_application::projects::{list_viewer_projects, status_index};

    let directory = tempfile::tempdir()?;
    let database = SqliteAppState::open(directory.path())?;
    let connection = database.connection_lock()?;
    register_viewer_project(
        &connection,
        "AA",
        "Original",
        "//fixture.invalid/repositories/repos/original",
    )?;
    connection.execute(
        "INSERT INTO project_sources (source_kind, source_value) VALUES ('directory', '//fixture.invalid/repositories/repos/replacement')",
        [],
    )?;
    let replacement = connection.last_insert_rowid();
    let original = list_viewer_projects::get_project(&"AA".try_into()?, &connection)?.unwrap();
    connection.execute(
        "UPDATE projects SET source_id = ?1 WHERE id = 'AA'",
        [replacement],
    )?;
    status_index::record(&original, None, &connection)?;
    let indexed: u32 =
        connection.query_row("SELECT count(*) FROM project_status_index", [], |row| {
            row.get(0)
        })?;
    assert_eq!(indexed, 0);
    let current = list_viewer_projects::get_project(&"AA".try_into()?, &connection)?.unwrap();
    status_index::record(&current, None, &connection)?;
    let indexed: u32 =
        connection.query_row("SELECT count(*) FROM project_status_index", [], |row| {
            row.get(0)
        })?;
    assert_eq!(indexed, 1);
    Ok(())
}
