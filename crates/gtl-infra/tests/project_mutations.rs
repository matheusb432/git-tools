use gtl_application::projects::{
    catalogue::{
        ProjectCatalogueError,
        create_project::{self, CreateProject},
        get_project,
        set_project_membership::{self, ProjectMembership, SetProjectMembership},
    },
    set_project_status::{self, SetProjectStatus},
};
use gtl_infra::app_state::SqliteAppState;
use gtl_models::projects::catalogue::{
    ProjectGroups, ProjectIds, ProjectMetadata, ProjectMutationOutcome, ProjectOperationMode,
    ProjectStatus,
};
use rusqlite::Connection;

const ORIGINAL_TIMESTAMP: &str = "2026-07-19T00:00:00.000Z";

fn create_project(connection: &mut Connection, id: &str) -> anyhow::Result<()> {
    create_project::execute(&project_request(id)?, connection)?;
    Ok(())
}

fn project_request(id: &str) -> anyhow::Result<CreateProject> {
    Ok(CreateProject {
        id: id.try_into()?,
        metadata: ProjectMetadata {
            title: id.try_into()?,
            source: std::env::temp_dir()
                .join(id)
                .to_string_lossy()
                .into_owned()
                .try_into()?,
            git_remote: None,
            color: None,
            groups: ProjectGroups::default(),
        },
        include_in_full_export: true,
    })
}

fn membership_request(
    ids: &[&str],
    managed: bool,
    mode: ProjectOperationMode,
) -> anyhow::Result<SetProjectMembership> {
    Ok(SetProjectMembership {
        ids: ProjectIds::try_new(
            ids.iter()
                .map(|id| (*id).try_into())
                .collect::<Result<_, _>>()?,
        )?,
        membership: if managed {
            ProjectMembership::Managed
        } else {
            ProjectMembership::Unmanaged
        },
        mode,
    })
}

#[test]
fn project_status_preserves_previews_and_idempotent_timestamps() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let mut connection = state.connection_lock()?;
    create_project(&mut connection, "GTL")?;
    for (initial_status, target_status) in [
        (ProjectStatus::Active, ProjectStatus::Active),
        (ProjectStatus::Active, ProjectStatus::Paused),
        (ProjectStatus::Paused, ProjectStatus::Active),
        (ProjectStatus::Paused, ProjectStatus::Paused),
    ] {
        for mode in [ProjectOperationMode::Preview, ProjectOperationMode::Apply] {
            let initial_timestamp =
                (initial_status == ProjectStatus::Paused).then_some(ORIGINAL_TIMESTAMP);
            connection.execute("UPDATE projects SET paused_at = ?1", [initial_timestamp])?;
            let changes_before = connection.total_changes();
            let request = SetProjectStatus {
                id: "GTL".try_into()?,
                status: target_status,
                mode,
            };
            let result = set_project_status::execute(&request, &mut connection)?;
            let changed = initial_status != target_status;
            assert_eq!(result.target_status, target_status);
            assert_eq!(
                result.outcome,
                if changed {
                    ProjectMutationOutcome::Changed
                } else {
                    ProjectMutationOutcome::Unchanged
                }
            );
            let applied = changed && mode == ProjectOperationMode::Apply;
            assert_eq!(
                connection.total_changes() - changes_before,
                u64::from(applied)
            );
            let stored = get_project::execute(&request.id, &connection)?;
            assert_eq!(
                stored.status,
                if applied {
                    target_status
                } else {
                    initial_status
                }
            );
            let timestamp: Option<String> =
                connection.query_row("SELECT paused_at FROM projects", [], |row| row.get(0))?;
            if !applied {
                assert_eq!(timestamp.as_deref(), initial_timestamp);
            }
        }
    }
    Ok(())
}

#[test]
fn project_membership_preserves_previews_timestamps_and_paused_state() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let mut connection = state.connection_lock()?;
    create_project(&mut connection, "GTL")?;
    connection.execute("UPDATE projects SET paused_at = ?1", [ORIGINAL_TIMESTAMP])?;
    for (initial_managed, target_managed) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        for mode in [ProjectOperationMode::Preview, ProjectOperationMode::Apply] {
            let initial_timestamp = (!initial_managed).then_some(ORIGINAL_TIMESTAMP);
            connection.execute("UPDATE projects SET unmanaged_at = ?1", [initial_timestamp])?;
            let changes_before = connection.total_changes();
            let request = membership_request(&["GTL"], target_managed, mode)?;
            let result = set_project_membership::execute(&request, &mut connection)?;
            let changed = initial_managed != target_managed;
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].id.as_ref(), "GTL");
            assert_eq!(
                result[0].outcome,
                if changed {
                    ProjectMutationOutcome::Changed
                } else {
                    ProjectMutationOutcome::Unchanged
                }
            );
            let applied = changed && mode == ProjectOperationMode::Apply;
            assert_eq!(
                connection.total_changes() - changes_before,
                u64::from(applied)
            );
            let (unmanaged_at, paused_at): (Option<String>, String) = connection.query_row(
                "SELECT unmanaged_at, paused_at FROM projects",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(
                unmanaged_at.is_none(),
                if applied {
                    target_managed
                } else {
                    initial_managed
                }
            );
            assert_eq!(paused_at, ORIGINAL_TIMESTAMP);
            if !applied {
                assert_eq!(unmanaged_at.as_deref(), initial_timestamp);
            }
        }
    }
    Ok(())
}

#[test]
fn project_status_rejects_missing_and_unmanaged_projects() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let mut connection = state.connection_lock()?;
    create_project(&mut connection, "GTL")?;
    connection.execute(
        "UPDATE projects SET unmanaged_at = ?1",
        [ORIGINAL_TIMESTAMP],
    )?;
    let changes_before = connection.total_changes();
    for id in ["GTL", "NONE"] {
        for status in [ProjectStatus::Active, ProjectStatus::Paused] {
            for mode in [ProjectOperationMode::Preview, ProjectOperationMode::Apply] {
                let result = set_project_status::execute(
                    &SetProjectStatus {
                        id: id.try_into()?,
                        status,
                        mode,
                    },
                    &mut connection,
                );
                assert!(matches!(result, Err(ProjectCatalogueError::NotFound)));
            }
        }
    }
    assert_eq!(connection.total_changes(), changes_before);
    Ok(())
}

#[test]
fn membership_batch_rolls_back_on_missing_project_and_keeps_request_order() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let mut connection = state.connection_lock()?;
    for id in ["AA", "BB"] {
        create_project(&mut connection, id)?;
    }
    for managed in [false, true] {
        let initial_timestamp = managed.then_some(ORIGINAL_TIMESTAMP);
        connection.execute("UPDATE projects SET unmanaged_at = ?1", [initial_timestamp])?;
        for mode in [ProjectOperationMode::Preview, ProjectOperationMode::Apply] {
            let request = membership_request(&["BB", "NONE"], managed, mode)?;
            assert!(matches!(
                set_project_membership::execute(&request, &mut connection),
                Err(ProjectCatalogueError::NotFound)
            ));
            let timestamp: Option<String> = connection.query_row(
                "SELECT unmanaged_at FROM projects WHERE id = 'BB'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(timestamp.as_deref(), initial_timestamp);
        }
        set_project_membership::execute(
            &membership_request(&["AA"], managed, ProjectOperationMode::Apply)?,
            &mut connection,
        )?;
        let result = set_project_membership::execute(
            &membership_request(&["BB", "AA"], managed, ProjectOperationMode::Apply)?,
            &mut connection,
        )?;
        assert_eq!(
            result
                .iter()
                .map(|item| (item.id.as_ref(), item.outcome))
                .collect::<Vec<_>>(),
            [
                ("BB", ProjectMutationOutcome::Changed),
                ("AA", ProjectMutationOutcome::Unchanged)
            ]
        );
    }
    Ok(())
}

#[test]
fn project_creation_binds_each_source_and_rolls_back_conflicts() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let mut connection = state.connection_lock()?;
    for id in ["AA", "BB"] {
        let request = project_request(id)?;
        let created = create_project::execute(&request, &mut connection)?;
        assert_eq!(created, request.id);
        assert_eq!(
            get_project::execute(&created, &connection)?.metadata,
            request.metadata
        );
    }
    for duplicate_source in [false, true] {
        let mut request = project_request("CC")?;
        if duplicate_source {
            request.metadata.source = project_request("AA")?.metadata.source;
        } else {
            request.metadata.title = "AA".try_into()?;
        }
        assert!(matches!(
            create_project::execute(&request, &mut connection),
            Err(ProjectCatalogueError::AlreadyExists)
        ));
        let sources: i64 =
            connection.query_row("SELECT count(*) FROM project_sources", [], |row| row.get(0))?;
        assert_eq!(sources, 2);
    }
    Ok(())
}

#[test]
fn membership_accepts_the_maximum_batch_and_returns_request_order() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let state = SqliteAppState::open(directory.path())?;
    let mut connection = state.connection_lock()?;
    let mut ids = Vec::new();
    for index in 0..gtl_models::projects::catalogue::PROJECT_MUTATIONS_MAX {
        let id = format!(
            "{}{}",
            char::from(b'A' + u8::try_from(index / 26)?),
            char::from(b'A' + u8::try_from(index % 26)?)
        );
        create_project(&mut connection, &id)?;
        ids.push(id);
    }
    ids.reverse();
    let request = membership_request(
        &ids.iter().map(String::as_str).collect::<Vec<_>>(),
        false,
        ProjectOperationMode::Apply,
    )?;
    let result = set_project_membership::execute(&request, &mut connection)?;
    assert_eq!(
        result
            .iter()
            .map(|mutation| mutation.id.as_ref())
            .collect::<Vec<_>>(),
        ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert!(
        result
            .iter()
            .all(|mutation| mutation.outcome == ProjectMutationOutcome::Changed)
    );
    let unmanaged: i64 = connection.query_row(
        "SELECT count(*) FROM projects WHERE unmanaged_at IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(unmanaged, i64::try_from(ids.len())?);
    Ok(())
}
