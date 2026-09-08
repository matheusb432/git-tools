use gtl_infra::app_state::SqliteAppState;
use gtl_wire::v1::{self, project_service_client::ProjectServiceClient};
use serial_test::serial;

use super::{ServerHarness, TestResult};

fn creation(id: &str, title: &str) -> v1::CreateProjectRequest {
    v1::CreateProjectRequest {
        project_id: id.into(),
        project: Some(v1::ProjectCreation {
            title: title.into(),
            source: Some(v1::ProjectSource {
                source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                    path: format!("~/tools/{id}"),
                })),
            }),
            git_remote: Some(format!("git@example.test:tools/{id}.git")),
            mux_session_name: id.to_ascii_lowercase(),
            affiliation: v1::ProjectAffiliation::Personal.into(),
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
    let mut projects =
        ProjectServiceClient::with_interceptor(server.native_channel(), server.authorization());
    projects
        .create_project(creation("TST", "Comparison test"))
        .await?;
    let mut viewer = v1::viewer_service_client::ViewerServiceClient::with_interceptor(
        server.native_channel(),
        server.authorization(),
    );
    let project = viewer
        .list_viewer_projects(v1::ListViewerProjectsRequest {})
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
        .list_viewer_projects(v1::ListViewerProjectsRequest {})
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
        .list_viewer_projects(v1::ListViewerProjectsRequest {})
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
async fn manages_its_own_projects_through_authenticated_grpc() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client =
        ProjectServiceClient::with_interceptor(server.native_channel(), server.authorization());
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
    assert_eq!(
        client
            .pause_project(pause(v1::ProjectOperationMode::Preview))
            .await?
            .into_inner()
            .outcome(),
        v1::ProjectMutationOutcome::Changed
    );
    assert_eq!(
        client
            .list_active_projects(v1::ListActiveProjectsRequest {})
            .await?
            .into_inner()
            .projects
            .len(),
        2
    );
    assert_eq!(
        client
            .pause_project(pause(v1::ProjectOperationMode::Apply))
            .await?
            .into_inner()
            .outcome(),
        v1::ProjectMutationOutcome::Changed
    );
    assert_eq!(
        client
            .pause_project(pause(v1::ProjectOperationMode::Apply))
            .await?
            .into_inner()
            .outcome(),
        v1::ProjectMutationOutcome::Unchanged
    );
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
    assert_eq!(
        client
            .resume_project(v1::ResumeProjectRequest {
                project_id: "GTL".into(),
                mode: v1::ProjectOperationMode::Apply.into()
            })
            .await?
            .into_inner()
            .outcome(),
        v1::ProjectMutationOutcome::Changed
    );
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
    let mut client = ProjectServiceClient::with_interceptor(
        restarted.native_channel(),
        restarted.authorization(),
    );
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
    let mut client =
        ProjectServiceClient::with_interceptor(server.native_channel(), server.authorization());
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
