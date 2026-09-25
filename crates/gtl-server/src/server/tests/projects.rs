use gtl_infra::app_state::SqliteAppState;
use gtl_wire::v1::{self, project_service_client::ProjectServiceClient};
use serial_test::serial;

use super::{ServerHarness, TestResult};

#[tokio::test]
#[serial(server_tracing)]
async fn viewer_discovers_and_imports_repositories_with_independent_row_results() -> TestResult {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("projects");
    for name in ["active", "paused", "unmanaged", "new", "bad"] {
        std::fs::create_dir_all(root.join(name).join(".git"))?;
    }
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut projects = ProjectServiceClient::new(server.native_channel());
    for (id, name) in [("ACT", "active"), ("PAU", "paused"), ("UNM", "unmanaged")] {
        let mut request = creation(id, name);
        request.project.as_mut().unwrap().source = Some(v1::ProjectSource {
            source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                path: root.join(name).to_string_lossy().into_owned(),
            })),
        });
        projects.create_project(request).await?;
    }
    projects
        .pause_project(v1::PauseProjectRequest {
            project_id: "PAU".into(),
            mode: v1::ProjectOperationMode::Apply as i32,
        })
        .await?;
    projects
        .unmanage_projects(v1::UnmanageProjectsRequest {
            project_ids: vec!["UNM".into()],
            mode: v1::ProjectOperationMode::Apply as i32,
        })
        .await?;

    let mut viewer = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let discover = || v1::DiscoverProjectRepositoriesRequest {
        root: root.to_string_lossy().into_owned(),
    };
    let found = viewer
        .discover_project_repositories(discover())
        .await?
        .into_inner();
    assert_eq!(found.root, root.canonicalize()?.to_string_lossy());
    assert_eq!(found.repositories.len(), 5);
    for (name, state, id) in [
        ("active", v1::ProjectDiscoveryState::Active, Some("ACT")),
        ("paused", v1::ProjectDiscoveryState::Paused, Some("PAU")),
        (
            "unmanaged",
            v1::ProjectDiscoveryState::Unmanaged,
            Some("UNM"),
        ),
        ("new", v1::ProjectDiscoveryState::New, None),
    ] {
        let repository = found
            .repositories
            .iter()
            .find(|repository| repository.label == name)
            .ok_or("missing discovered repository")?;
        assert_eq!(repository.state, state as i32);
        assert_eq!(repository.existing_project_id.as_deref(), id);
    }
    let database = SqliteAppState::open(directory.path())?;
    {
        let connection = database.connection_lock()?;
        connection.execute(
            "INSERT INTO render_sources (id, kind, value, created_at) VALUES (1, 'directory', ?1, '2026-01-01T00:00:00Z')",
            [root.join("new").to_string_lossy().into_owned()],
        )?;
        connection.execute_batch(
            "INSERT INTO recent_renders
             (id, source_id, operation_id, target_id, recipe_name, repo_name, range_label, rendered_at)
             VALUES (1, 1, 1, 1, 'Snapshot', 'new', 'main..HEAD', '2026-01-01T00:00:00Z')",
        )?;
    }
    let results = viewer
        .import_project_repositories(v1::ImportProjectRepositoriesRequest {
            selections: vec![
                v1::ProjectImportSelection {
                    path: root.join("new").to_string_lossy().into_owned(),
                    project_id: "NEW".into(),
                    title: "New project".into(),
                },
                v1::ProjectImportSelection {
                    path: root.join("unmanaged").to_string_lossy().into_owned(),
                    project_id: "UNM".into(),
                    title: String::new(),
                },
                v1::ProjectImportSelection {
                    path: root.join("bad").to_string_lossy().into_owned(),
                    project_id: "bad".into(),
                    title: "Bad project".into(),
                },
            ],
        })
        .await?
        .into_inner()
        .results;
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].outcome, v1::ProjectImportOutcome::Created as i32);
    assert_eq!(
        results[1].outcome,
        v1::ProjectImportOutcome::Restored as i32
    );
    assert_eq!(results[2].outcome, v1::ProjectImportOutcome::Failed as i32);
    assert_eq!(
        results[2]
            .failure
            .clone()
            .and_then(gtl_wire::proto::failure::decode_failure),
        Some(gtl_models::failure::Failure::InvalidRequest {
            field: "project_id".into()
        })
    );
    let created = projects
        .get_project(v1::GetProjectRequest {
            project_id: "NEW".into(),
        })
        .await?
        .into_inner();
    assert_eq!(created.git_remote, None);
    assert_eq!(created.title, "New project");
    assert_eq!(
        database.connection_lock()?.query_row(
            "SELECT project_id FROM recent_renders WHERE id = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )?,
        Some("NEW".to_owned())
    );
    assert_eq!(
        created.source.unwrap().source.unwrap(),
        v1::project_source::Source::Directory(v1::DirectorySource {
            path: root.join("new").to_string_lossy().into_owned(),
        })
    );
    projects
        .get_project(v1::GetProjectRequest {
            project_id: "UNM".into(),
        })
        .await?;
    assert_eq!(
        projects
            .get_project(v1::GetProjectRequest {
                project_id: "BAD".into(),
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::NotFound
    );
    assert_eq!(
        viewer
            .discover_project_repositories(v1::DiscoverProjectRepositoriesRequest {
                root: "relative/folder".into(),
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn project_status_watch_coalesces_edits_ignores_builds_and_releases_on_disconnect()
-> TestResult {
    use std::{process::Command, time::Duration};

    use gtl_wire::{
        proto::viewer::projects::decode_status_update, viewer::projects::ViewerProjectStatusUpdate,
    };
    async fn next(
        stream: &mut tonic::Streaming<v1::WatchViewerResponse>,
    ) -> Result<ViewerProjectStatusUpdate, Box<dyn std::error::Error>> {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let event = stream.message().await?.ok_or("watch ended")?;
                if let Some(update) = event.project_status {
                    return Ok(decode_status_update(update)?);
                }
            }
        })
        .await?
    }
    let home = directories::BaseDirs::new().ok_or("home unavailable")?;
    let repo = tempfile::Builder::new()
        .prefix(".gtl-watch-")
        .tempdir_in(home.home_dir())?;
    let git = |args: &[&str]| -> TestResult {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(args)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    };
    git(&["init", "-q", "-b", "main"])?;
    std::fs::write(repo.path().join(".gitignore"), "/target/\n")?;
    std::fs::write(repo.path().join("file"), "initial\n")?;
    git(&["add", "."])?;
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.test",
        "commit",
        "-qm",
        "initial",
    ])?;
    std::fs::create_dir_all(repo.path().join("target/debug/deps"))?;
    let data = tempfile::tempdir()?;
    let server = ServerHarness::start(data.path(), None).await?;
    let mut projects = ProjectServiceClient::new(server.native_channel());
    let mut create = creation("TST", "Status watch");
    create.project.as_mut().unwrap().source = Some(v1::ProjectSource {
        source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
            path: repo.path().to_string_lossy().into_owned(),
        })),
    });
    projects.create_project(create).await?;
    let mut viewer = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    for ids in [
        vec!["TST".into(), "TST".into()],
        vec!["bad".into()],
        vec!["TST".into(); 101],
    ] {
        assert_eq!(
            viewer
                .watch_viewer(v1::WatchViewerRequest {
                    live_tab_id: None,
                    project_ids: ids
                })
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }
    let request = v1::WatchViewerRequest {
        live_tab_id: None,
        project_ids: vec!["TST".into()],
    };
    let mut stream = viewer.watch_viewer(request.clone()).await?.into_inner();
    let ViewerProjectStatusUpdate::Status(initial) = next(&mut stream).await? else {
        return Err("initial status unavailable".into());
    };
    let before = server.project_status_observations();
    assert_eq!(before.0, 1);
    assert!(before.1 > 0);
    for index in 0..500 {
        std::fs::write(
            repo.path().join(format!("target/debug/deps/{index}")),
            "build",
        )?;
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        server.project_status_observations().0,
        before.0,
        "ignored builds must not run status"
    );
    for index in 0..20 {
        std::fs::write(repo.path().join("file"), format!("edit {index}\n"))?;
    }
    let ViewerProjectStatusUpdate::Status(changed) = next(&mut stream).await? else {
        return Err("changed status unavailable".into());
    };
    assert_ne!(initial.status, changed.status);
    assert_eq!(
        server.project_status_observations().0,
        before.0 + 1,
        "save burst must coalesce"
    );
    git(&["checkout", "-qb", "feature"])?;
    let ViewerProjectStatusUpdate::Status(branch) = next(&mut stream).await? else {
        return Err("branch status unavailable".into());
    };
    assert!(
        matches!(branch.status, gtl_models::repository::status::RepositoryStatus::Present { head: gtl_models::repository::status::StatusHead::Branch { name, .. }, .. } if name.as_str() == "feature")
    );
    drop(stream);
    tokio::time::timeout(Duration::from_secs(5), async {
        while server.project_status_observations().1 != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    std::fs::write(repo.path().join("untracked"), "new")?;
    let mut stream = viewer.watch_viewer(request).await?.into_inner();
    let _cached = next(&mut stream).await?;
    let ViewerProjectStatusUpdate::Status(fresh) = next(&mut stream).await? else {
        return Err("resumed status unavailable".into());
    };
    assert!(
        matches!(fresh.status, gtl_models::repository::status::RepositoryStatus::Present { changes: gtl_models::repository::status::StatusChanges::Changed { untracked, .. }, .. } if !untracked.is_zero())
    );
    drop(stream);
    server.stop().await?;
    Ok(())
}

fn creation(id: &str, title: &str) -> v1::CreateProjectRequest {
    v1::CreateProjectRequest {
        project_id: id.into(),
        project: Some(v1::ProjectCreation {
            title: title.into(),
            source: Some(v1::ProjectSource {
                source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                    path: std::env::temp_dir()
                        .join("tools")
                        .join(id)
                        .to_string_lossy()
                        .into_owned(),
                })),
            }),
            git_remote: Some(format!("git@example.test:tools/{id}.git")),
            color: Some("#112233".into()),
            groups: vec!["tools".into()],
            include_in_full_export: None,
        }),
    }
}

#[tokio::test]
#[serial(server_tracing)]
async fn updates_project_comparison_with_validation_and_a_revision_precondition() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut projects = ProjectServiceClient::new(server.native_channel());
    projects
        .create_project(creation("TST", "Comparison test"))
        .await?;
    let mut viewer = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let project = viewer
        .list_viewer_projects(page_request(
            15,
            v1::list_viewer_projects_request::Cursor::First(v1::Empty {}),
        ))
        .await?
        .into_inner()
        .projects
        .remove(0);
    assert_eq!(project.comparison_branch, "main");
    let request = |branch: &str| v1::UpdateViewerProjectRequest {
        path: project.path.clone(),
        expected_comparison_branch: "main".into(),
        comparison_branch: Some(v1::ComparisonBranchFieldUpdate {
            operation: Some(v1::comparison_branch_field_update::Operation::Update(
                branch.into(),
            )),
        }),
    };
    assert_eq!(
        viewer
            .update_viewer_project(request("main~1"))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    viewer.update_viewer_project(request("develop")).await?;
    assert_eq!(
        viewer
            .update_viewer_project(request("release"))
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Aborted
    );
    let updated = viewer
        .list_viewer_projects(page_request(
            15,
            v1::list_viewer_projects_request::Cursor::First(v1::Empty {}),
        ))
        .await?
        .into_inner()
        .projects
        .remove(0);
    assert_eq!(updated.comparison_branch, "develop");
    viewer
        .update_viewer_project(v1::UpdateViewerProjectRequest {
            path: project.path,
            expected_comparison_branch: "develop".into(),
            comparison_branch: Some(v1::ComparisonBranchFieldUpdate {
                operation: Some(v1::comparison_branch_field_update::Operation::Clear(
                    v1::ClearSetting {},
                )),
            }),
        })
        .await?;
    let updated = viewer
        .list_viewer_projects(page_request(
            15,
            v1::list_viewer_projects_request::Cursor::First(v1::Empty {}),
        ))
        .await?
        .into_inner()
        .projects
        .remove(0);
    assert_eq!(updated.comparison_branch, "main");
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn manages_its_own_projects_through_private_grpc() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = ProjectServiceClient::new(server.native_channel());
    let response = client
        .create_project(creation("GTL", "Git Tools"))
        .await?
        .into_inner();
    assert_eq!(response.project_id, "GTL");
    client.create_project(creation("APP", "sample_project")).await?;
    let projects = client
        .list_active_projects(v1::ListActiveProjectsRequest {})
        .await?
        .into_inner()
        .projects;
    assert_eq!(
        projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>(),
        ["APP", "GTL"]
    );
    let project = client
        .get_project(v1::GetProjectRequest {
            project_id: "GTL".into(),
        })
        .await?
        .into_inner();
    assert_eq!(project.title, "Git Tools");
    assert_eq!(project.groups, ["tools"]);
    assert_eq!(project.color.as_deref(), Some("#112233"));
    assert_eq!(
        project.git_remote.as_deref(),
        Some("git@example.test:tools/GTL.git")
    );
    assert_eq!(project.status(), v1::ProjectStatus::Active);
    let pause = |mode: v1::ProjectOperationMode| v1::PauseProjectRequest {
        project_id: "GTL".into(),
        mode: mode.into(),
    };
    let preview = client
        .pause_project(pause(v1::ProjectOperationMode::Preview))
        .await?
        .into_inner();
    assert_eq!(preview.outcome(), v1::ProjectMutationOutcome::Changed);
    assert_eq!(preview.target_status(), v1::ProjectStatus::Paused);
    assert_eq!(
        client
            .list_active_projects(v1::ListActiveProjectsRequest {})
            .await?
            .into_inner()
            .projects
            .len(),
        2
    );
    let paused = client
        .pause_project(pause(v1::ProjectOperationMode::Apply))
        .await?
        .into_inner();
    assert_eq!(paused.outcome(), v1::ProjectMutationOutcome::Changed);
    assert_eq!(paused.target_status(), v1::ProjectStatus::Paused);
    let unchanged = client
        .pause_project(pause(v1::ProjectOperationMode::Apply))
        .await?
        .into_inner();
    assert_eq!(unchanged.outcome(), v1::ProjectMutationOutcome::Unchanged);
    assert_eq!(unchanged.target_status(), v1::ProjectStatus::Paused);
    assert_eq!(
        client
            .list_active_projects(v1::ListActiveProjectsRequest {})
            .await?
            .into_inner()
            .projects
            .len(),
        1
    );
    assert_eq!(
        client
            .get_project(v1::GetProjectRequest {
                project_id: "GTL".into()
            })
            .await?
            .into_inner()
            .status(),
        v1::ProjectStatus::Paused
    );
    let unmanage = |ids: Vec<String>, mode: v1::ProjectOperationMode| v1::UnmanageProjectsRequest {
        project_ids: ids,
        mode: mode.into(),
    };
    client
        .unmanage_projects(unmanage(
            vec!["GTL".into()],
            v1::ProjectOperationMode::Preview,
        ))
        .await?;
    client
        .get_project(v1::GetProjectRequest {
            project_id: "GTL".into(),
        })
        .await?;
    client
        .unmanage_projects(unmanage(
            vec!["GTL".into()],
            v1::ProjectOperationMode::Apply,
        ))
        .await?;
    let error = client
        .get_project(v1::GetProjectRequest {
            project_id: "GTL".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
    let error = client
        .resume_project(v1::ResumeProjectRequest {
            project_id: "GTL".into(),
            mode: v1::ProjectOperationMode::Apply.into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
    client
        .manage_projects(v1::ManageProjectsRequest {
            project_ids: vec!["GTL".into()],
            mode: v1::ProjectOperationMode::Apply.into(),
        })
        .await?;
    assert_eq!(
        client
            .get_project(v1::GetProjectRequest {
                project_id: "GTL".into()
            })
            .await?
            .into_inner()
            .status(),
        v1::ProjectStatus::Paused
    );
    let resumed = client
        .resume_project(v1::ResumeProjectRequest {
            project_id: "GTL".into(),
            mode: v1::ProjectOperationMode::Apply.into(),
        })
        .await?
        .into_inner();
    assert_eq!(resumed.outcome(), v1::ProjectMutationOutcome::Changed);
    assert_eq!(resumed.target_status(), v1::ProjectStatus::Active);
    // The first update must roll back when a later ID is missing.
    let error = client
        .unmanage_projects(unmanage(
            vec!["GTL".into(), "ZZ".into()],
            v1::ProjectOperationMode::Apply,
        ))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
    client
        .get_project(v1::GetProjectRequest {
            project_id: "GTL".into(),
        })
        .await?;
    server.stop().await?;
    let restarted = ServerHarness::start(directory.path(), None).await?;
    let mut client = ProjectServiceClient::new(restarted.native_channel());
    assert_eq!(
        client
            .list_active_projects(v1::ListActiveProjectsRequest {})
            .await?
            .into_inner()
            .projects
            .len(),
        2
    );
    restarted.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn rejects_invalid_project_requests_and_rolls_back_duplicate_creation() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = ProjectServiceClient::new(server.native_channel());
    client.create_project(creation("GTL", "Git Tools")).await?;
    for request in [creation("GTL", "Another"), creation("APP", "git tools")] {
        let error = client.create_project(request).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::AlreadyExists);
    }
    let missing = v1::CreateProjectRequest {
        project_id: "APP".into(),
        project: None,
    };
    assert_eq!(
        client.create_project(missing).await.unwrap_err().code(),
        tonic::Code::InvalidArgument
    );
    let mut request = creation("APP", "sample_project");
    request.project.as_mut().unwrap().source = Some(v1::ProjectSource {
        source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
            path: "~/../outside".into(),
        })),
    });
    assert_eq!(
        client.create_project(request).await.unwrap_err().code(),
        tonic::Code::InvalidArgument
    );
    assert_eq!(
        client
            .pause_project(v1::PauseProjectRequest {
                project_id: "GTL".into(),
                mode: 99
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    for ids in [
        vec![],
        vec!["GTL".into(), "GTL".into()],
        vec!["GTL".into(); 257],
    ] {
        assert_eq!(
            client
                .manage_projects(v1::ManageProjectsRequest {
                    project_ids: ids,
                    mode: v1::ProjectOperationMode::Apply.into()
                })
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }
    let database = SqliteAppState::open(directory.path())?;
    let connection = database.connection_lock()?;
    let sources: i64 =
        connection.query_row("SELECT count(*) FROM project_sources", [], |row| row.get(0))?;
    assert_eq!(sources, 1);
    let export: bool = connection.query_row(
        "SELECT export_include_in_all FROM projects WHERE id = 'GTL'",
        [],
        |row| row.get(0),
    )?;
    assert!(export);
    let foreign_key_failures: i64 =
        connection.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    assert_eq!(foreign_key_failures, 0);
    drop(connection);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn resolves_managed_project_sources_including_paused_projects() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = ProjectServiceClient::new(server.native_channel());
    client
        .create_project(creation("DEMO", "Example project"))
        .await?;
    let request = || v1::GetProjectRepositoryRequest {
        project_id: "DEMO".into(),
    };
    let expected = std::env::temp_dir()
        .join("tools/DEMO")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        client
            .get_project_repository(request())
            .await?
            .into_inner()
            .repository_root,
        expected
    );

    client
        .pause_project(v1::PauseProjectRequest {
            project_id: "DEMO".into(),
            mode: v1::ProjectOperationMode::Apply.into(),
        })
        .await?;
    assert_eq!(
        client
            .get_project_repository(request())
            .await?
            .into_inner()
            .repository_root,
        expected
    );
    assert!(
        client
            .list_active_projects(v1::ListActiveProjectsRequest {})
            .await?
            .into_inner()
            .projects
            .is_empty()
    );

    client
        .unmanage_projects(v1::UnmanageProjectsRequest {
            project_ids: vec!["DEMO".into()],
            mode: v1::ProjectOperationMode::Apply.into(),
        })
        .await?;
    assert_eq!(
        client
            .get_project_repository(request())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::NotFound
    );
    assert_eq!(
        client
            .get_project_repository(v1::GetProjectRepositoryRequest {
                project_id: "invalid-id".into()
            })
            .await
            .unwrap_err()
            .code(),
        tonic::Code::InvalidArgument
    );
    server.stop().await?;
    Ok(())
}

fn page_request(
    page_size: u32,
    cursor: v1::list_viewer_projects_request::Cursor,
) -> v1::ListViewerProjectsRequest {
    v1::ListViewerProjectsRequest {
        sort: None,
        page_size,
        cursor: Some(cursor),
    }
}

#[tokio::test]
#[serial(server_tracing)]
async fn viewer_projects_require_id_cursors_and_fetch_status_independently() -> TestResult {
    use v1::list_viewer_projects_request::Cursor;

    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut projects = ProjectServiceClient::new(server.native_channel());
    let mut viewer = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let first = || page_request(2, Cursor::First(v1::Empty {}));
    let empty = viewer.list_viewer_projects(first()).await?.into_inner();
    assert!(empty.projects.is_empty());
    assert_eq!(empty.total, 0);
    for (id, name) in [
        ("EE", "First title"),
        ("AA", "Last title"),
        ("DD", "Fourth"),
        ("BB", "Second"),
        ("CC", "Third"),
    ] {
        projects.create_project(creation(id, name)).await?;
    }
    let ids = |page: &v1::ListViewerProjectsResponse| {
        page.projects
            .iter()
            .map(|project| project.id.clone())
            .collect::<Vec<_>>()
    };
    let page = viewer.list_viewer_projects(first()).await?.into_inner();
    assert_eq!(ids(&page), ["AA", "BB"]);
    assert_eq!((page.total, page.count_before), (5, 0));
    let named = viewer
        .list_viewer_projects(v1::ListViewerProjectsRequest {
            sort: Some(v1::ProjectsSort::Name as i32),
            ..first()
        })
        .await?
        .into_inner();
    assert_eq!(ids(&named), ["EE", "DD"]);
    let named_next = viewer
        .list_viewer_projects(v1::ListViewerProjectsRequest {
            sort: Some(v1::ProjectsSort::Name as i32),
            ..page_request(2, Cursor::AfterProjectId("DD".into()))
        })
        .await?
        .into_inner();
    assert_eq!(ids(&named_next), ["AA", "BB"]);
    assert_eq!(named_next.count_before, 2);
    let page = viewer
        .list_viewer_projects(page_request(2, Cursor::AfterProjectId("BB".into())))
        .await?
        .into_inner();
    assert_eq!(ids(&page), ["CC", "DD"]);
    assert_eq!(page.count_before, 2);
    let last = viewer
        .list_viewer_projects(page_request(2, Cursor::Last(v1::Empty {})))
        .await?
        .into_inner();
    assert_eq!(ids(&last), ["EE"]);
    assert_eq!(last.count_before, 4);
    let previous = viewer
        .list_viewer_projects(page_request(2, Cursor::BeforeProjectId("EE".into())))
        .await?
        .into_inner();
    assert_eq!(ids(&previous), ["CC", "DD"]);
    let previous = viewer
        .list_viewer_projects(page_request(2, Cursor::BeforeProjectId("CC".into())))
        .await?
        .into_inner();
    assert_eq!(ids(&previous), ["AA", "BB"]);
    assert!(
        viewer
            .list_viewer_projects(page_request(2, Cursor::AfterProjectId("EE".into())))
            .await?
            .into_inner()
            .projects
            .is_empty()
    );
    for request in [
        page_request(0, Cursor::First(v1::Empty {})),
        page_request(101, Cursor::First(v1::Empty {})),
        page_request(u32::MAX, Cursor::First(v1::Empty {})),
        page_request(2, Cursor::AfterProjectId(String::new())),
        page_request(2, Cursor::BeforeProjectId("invalid".into())),
        v1::ListViewerProjectsRequest {
            sort: None,
            page_size: 2,
            cursor: None,
        },
    ] {
        assert_eq!(
            viewer
                .list_viewer_projects(request)
                .await
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }
    let workers = server
        .project_status_workers()
        .acquire_many_owned(4)
        .await?;
    let mut status_client = viewer.clone();
    let mut pending = tokio::spawn(async move {
        status_client
            .get_viewer_project_status(v1::GetViewerProjectStatusRequest {
                project_id: "AA".into(),
            })
            .await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut pending)
            .await
            .is_err()
    );
    let page = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        viewer.list_viewer_projects(first()),
    )
    .await??
    .into_inner();
    assert_eq!(ids(&page), ["AA", "BB"]);
    drop(workers);
    assert_eq!(pending.await??.into_inner().project_id, "AA");
    let status = viewer
        .get_viewer_project_status(v1::GetViewerProjectStatusRequest {
            project_id: "AA".into(),
        })
        .await?
        .into_inner();
    assert_eq!(status.project_id, "AA");
    assert!(matches!(
        status.status,
        Some(v1::get_viewer_project_status_response::Status::Absent(_))
    ));
    for (id, code) in [
        ("invalid", tonic::Code::InvalidArgument),
        ("ZZ", tonic::Code::NotFound),
    ] {
        assert_eq!(
            viewer
                .get_viewer_project_status(v1::GetViewerProjectStatusRequest {
                    project_id: id.into()
                })
                .await
                .unwrap_err()
                .code(),
            code
        );
    }
    projects
        .pause_project(v1::PauseProjectRequest {
            project_id: "CC".into(),
            mode: v1::ProjectOperationMode::Apply.into(),
        })
        .await?;
    projects
        .unmanage_projects(v1::UnmanageProjectsRequest {
            project_ids: vec!["BB".into()],
            mode: v1::ProjectOperationMode::Apply.into(),
        })
        .await?;
    let page = viewer
        .list_viewer_projects(page_request(2, Cursor::AfterProjectId("BB".into())))
        .await?
        .into_inner();
    assert_eq!(ids(&page), ["DD", "EE"]);
    assert_eq!(page.total, 3);
    for id in ["BB", "CC"] {
        assert_eq!(
            viewer
                .get_viewer_project_status(v1::GetViewerProjectStatusRequest {
                    project_id: id.into()
                })
                .await
                .unwrap_err()
                .code(),
            tonic::Code::NotFound
        );
    }
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn project_status_watch_bounds_waiting_pages_and_cancels_the_previous_page() -> TestResult {
    let data = tempfile::tempdir()?;
    let server = ServerHarness::start(data.path(), None).await?;
    let mut viewer = v1::viewer_service_client::ViewerServiceClient::new(server.native_channel());
    let request = v1::WatchViewerRequest {
        live_tab_id: None,
        project_ids: vec!["TST".into()],
    };
    let first = viewer.watch_viewer(request.clone()).await?.into_inner();
    let mut waiting = viewer.watch_viewer(request.clone()).await?.into_inner();
    assert_eq!(
        viewer.watch_viewer(request).await.unwrap_err().code(),
        tonic::Code::ResourceExhausted
    );
    drop(first);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = waiting.message().await?.ok_or("watch ended")?;
            if let Some(update) = event.project_status {
                assert_eq!(update.project_id, "TST");
                return Ok::<_, Box<dyn std::error::Error>>(());
            }
        }
    })
    .await??;
    drop(waiting);
    server.stop().await?;
    Ok(())
}
