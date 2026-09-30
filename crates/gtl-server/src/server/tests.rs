mod commit_search;
mod file_filters;
mod live_views;
mod projects;
mod repositories;
mod row_sessions;
mod viewer_push;
mod viewer_tabs;

use std::{error::Error, time::Duration};

use gtl_wire::v1::{
    BoolFieldUpdate, DiffTarget, EditSettingsRequest, Empty, GetRecursiveRepositoryStatusesRequest,
    GetRepositoryStatusRequest, GetViewerServerInfoRequest, GetViewerSettingsRequest,
    GetViewerShellRequest, MoveViewerTabRequest, PushProjectRepositoriesRequest, RenderDiffRequest,
    SetViewerThemeRequest, ViewerTabPlacement, ViewerTheme, WatchViewerRequest, bool_field_update,
    diff_service_client::DiffServiceClient, diff_target,
    project_service_client::ProjectServiceClient,
    repository_service_client::RepositoryServiceClient,
    settings_service_client::SettingsServiceClient, viewer_service_client::ViewerServiceClient,
};
use prost::Message as _;
use prost_types::FileDescriptorProto;
use serde_json::Value;
use serial_test::serial;
use tokio_stream::StreamExt as _;
use tonic::{Request, transport::Channel};
use tonic_health::{
    ServingStatus,
    pb::{HealthCheckRequest, health_client::HealthClient},
};
use tonic_reflection::pb::v1::{
    ServerReflectionRequest, ServerReflectionResponse,
    server_reflection_client::ServerReflectionClient, server_reflection_request::MessageRequest,
    server_reflection_response::MessageResponse,
};

use crate::{
    harness::ServerHarness,
    observability::{build_test_dispatch, read_json_records},
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

async fn wait_for_history(client: &mut ViewerServiceClient<Channel>) -> TestResult {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let history = client
                .list_viewer_history(gtl_wire::v1::ListViewerHistoryRequest {
                    filter: Some(
                        gtl_wire::v1::list_viewer_history_request::Filter::AllProjects(Empty {}),
                    ),
                    cursor: Some(gtl_wire::v1::list_viewer_history_request::Cursor::Newest(
                        Empty {},
                    )),
                })
                .await?
                .into_inner();
            if !history.entries.is_empty() {
                return Ok::<(), tonic::Status>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await??;
    Ok(())
}

/// Saves `recipes` as live tabs after any saved ones, as a previous server would, leaving the
/// last one active.
fn seed_live_tabs(
    database: &gtl_infra::app_state::SqliteAppState,
    recipes: impl IntoIterator<Item = gtl_application::recipes::Recipe>,
) -> TestResult {
    use gtl_application::viewer::saved_tabs::{self, SavedViewerTab};
    let mut connection = database.connection_lock()?;
    let mut tabs = saved_tabs::load(&connection)?;
    for tab in &mut tabs {
        tab.active = false;
    }
    tabs.extend(recipes.into_iter().map(|recipe| SavedViewerTab {
        history_id: None,
        comparison_name: None,
        label: gtl_models::recipes::RecipeLabel::Repository {
            repository: recipe.cwd().project_name(),
        },
        recipe,
        pinned: false,
        live: true,
        active: false,
    }));
    if let Some(last) = tabs.last_mut() {
        last.active = true;
    }
    saved_tabs::save(&mut connection, &tabs)?;
    Ok(())
}

/// Compares the working tree of `repository` with its `HEAD`.
fn working_tree_recipe(
    repository: gtl_models::paths::RepositoryRoot,
) -> gtl_application::recipes::Recipe {
    use gtl_application::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};
    Recipe {
        source: RecipeSource::LocalRepo(repository),
        op: RecipeOp::Diff {
            target: RecipeTarget::Base {
                rev: gtl_models::git::GitRevision::head(),
            },
        },
        name: None,
    }
}

/// Compares `repository` with its upstream or comparison branch.
fn unpushed_recipe(
    repository: gtl_models::paths::RepositoryRoot,
) -> gtl_application::recipes::Recipe {
    use gtl_application::recipes::{Recipe, RecipeOp, RecipeSource, RecipeTarget};
    Recipe {
        source: RecipeSource::LocalRepo(repository),
        op: RecipeOp::Diff {
            target: RecipeTarget::Unpushed { pinned: None },
        },
        name: None,
    }
}

const PRIVATE_METADATA_VALUE: &str = "gtl-observability-private-metadata";
const DIFF_RENDER_URI: &str = "/gtl.v1.DiffService/RenderDiff";
const HEALTH_CHECK_URI: &str = "/grpc.health.v1.Health/Check";
const REFLECTION_URI: &str = "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo";

#[tokio::test]
#[serial(server_tracing)]
async fn serves_health_and_reflection() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;

    assert_health_serving(server.native_channel()).await?;
    assert_reflection_describes_gtl_contract(server.native_channel()).await?;

    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn validates_application_requests_through_the_generated_client() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = DiffServiceClient::new(server.native_channel());

    let error = client
        .render_diff(relative_working_directory_diff_request())
        .await
        .unwrap_err();

    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn rejects_retired_diff_settings_before_managed_push_dependencies() -> TestResult {
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("config.toml");
    std::fs::write(&settings_path, "[diff]\nexclude = [\"md\"]\n")?;
    let server = ServerHarness::start(directory.path(), Some(settings_path.clone())).await?;
    let mut client = ProjectServiceClient::new(server.native_channel());

    let error = client
        .push_project_repositories(PushProjectRepositoriesRequest { dry_run: true })
        .await
        .unwrap_err();

    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert!(
        error
            .message()
            .contains(&settings_path.display().to_string())
    );
    assert!(error.message().contains("unknown field `diff`"));
    assert!(matches!(
        gtl_wire::proto::failure::decode_status(&error),
        gtl_wire::proto::failure::StatusFailure::Decoded(gtl_models::failure::Failure::Settings(
            gtl_models::failure::SettingsFailure::Invalid { .. }
        ))
    ));
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn reports_invalid_viewer_keybinding_with_the_config_path() -> TestResult {
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("private-config.toml");
    std::fs::write(&settings_path, "[keybindings]\nsearch_files = \"Cmd+P\"\n")?;
    let server = ServerHarness::start(directory.path(), Some(settings_path.clone())).await?;
    let mut client = ViewerServiceClient::new(server.native_channel());

    let error = client
        .get_viewer_shell(GetViewerShellRequest {})
        .await
        .unwrap_err();

    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert!(
        error
            .message()
            .contains("`keybindings.search_files` is invalid: unsupported token `Cmd`")
    );
    assert!(
        error
            .message()
            .contains(&settings_path.display().to_string())
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn invalid_settings_can_be_inspected_and_reset_without_restarting_the_server() -> TestResult {
    use gtl_wire::v1::{GetSettingsRecoveryRequest, ResetSettingsRequest};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    let raw = "[[projects]]\nname = \"rust-snake\"\nexclude_from_push_all = true\n";
    std::fs::write(&path, raw)?;
    let server = ServerHarness::start(directory.path(), Some(path.clone())).await?;
    let mut client = ViewerServiceClient::new(server.native_channel());
    let error = client
        .get_viewer_shell(GetViewerShellRequest {})
        .await
        .unwrap_err();
    assert!(
        error
            .message()
            .contains("unknown field `exclude_from_push_all`")
    );
    let recovery = client
        .get_settings_recovery(GetSettingsRecoveryRequest {})
        .await?
        .into_inner();
    assert_eq!(recovery.configuration_path, path.display().to_string());
    assert!(
        recovery
            .diagnostic
            .as_ref()
            .unwrap()
            .contains("excluded_from_push_all")
    );
    let stale = ResetSettingsRequest {
        revision: "0".repeat(64),
    };
    assert_eq!(
        client.reset_settings(stale).await.unwrap_err().code(),
        tonic::Code::Aborted
    );
    assert_eq!(std::fs::read_to_string(&path)?, raw);
    let request = ResetSettingsRequest {
        revision: recovery.revision,
    };
    let reset = client.reset_settings(request.clone()).await?.into_inner();
    assert_eq!(std::fs::read_to_string(&reset.backup_path)?, raw);
    assert_eq!(std::fs::read_to_string(&path)?, "");
    client.get_viewer_shell(GetViewerShellRequest {}).await?;
    assert!(
        client
            .get_settings_recovery(GetSettingsRecoveryRequest {})
            .await?
            .into_inner()
            .diagnostic
            .is_none()
    );
    assert_eq!(
        client.reset_settings(request).await.unwrap_err().code(),
        tonic::Code::Aborted
    );
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn projects_configured_keybindings_into_the_viewer_shell() -> TestResult {
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("config.toml");
    std::fs::write(
        &settings_path,
        r#"
[keybindings]
search_files = "alt+k"
search_text_in_all_files = "ctrl+shift+g"
"#,
    )?;
    let server = ServerHarness::start(directory.path(), Some(settings_path)).await?;
    let mut client = ViewerServiceClient::new(server.native_channel());

    let response = client
        .get_viewer_shell(GetViewerShellRequest {})
        .await?
        .into_inner();
    let keybindings = response
        .shell
        .and_then(|shell| shell.preferences)
        .and_then(|preferences| preferences.keybindings)
        .ok_or("viewer shell omitted keybindings")?;

    assert_eq!(keybindings.search_files, "alt+k");
    assert_eq!(keybindings.search_text_in_all_files, "ctrl+shift+g");
    assert_ne!(keybindings.platform, 0);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn maps_repository_discovery_failures_to_grpc_statuses() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = RepositoryServiceClient::new(server.native_channel());
    let root = directory.path().to_string_lossy().into_owned();

    let error = client
        .get_repository_status(GetRepositoryStatusRequest {
            repository_path: root.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);

    let error = client
        .get_recursive_repository_statuses(GetRecursiveRepositoryStatusesRequest { root })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn viewer_server_info_is_stable_until_the_server_is_replaced() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = ViewerServiceClient::new(server.native_channel());
    let first = client
        .get_viewer_server_info(GetViewerServerInfoRequest {})
        .await?
        .into_inner();
    let repeated = client
        .get_viewer_server_info(GetViewerServerInfoRequest {})
        .await?
        .into_inner();
    assert_eq!(first, repeated);
    uuid::Uuid::parse_str(&first.server_instance_id)?;
    assert_eq!(
        first.protocol_version,
        gtl_wire::viewer::VIEWER_PROTOCOL_VERSION
    );
    server.stop().await?;

    let replacement = ServerHarness::start(directory.path(), None).await?;
    let replacement_info = ViewerServiceClient::new(replacement.native_channel())
        .get_viewer_server_info(GetViewerServerInfoRequest {})
        .await?
        .into_inner();
    assert_ne!(
        replacement_info.server_instance_id,
        first.server_instance_id
    );
    replacement.stop().await?;
    Ok(())
}

#[tokio::test]
async fn move_viewer_tab_validates_identity_through_the_generated_client() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut viewer = ViewerServiceClient::new(server.native_channel());

    let error = viewer
        .move_viewer_tab(MoveViewerTabRequest {
            tab_id: 1,
            target_tab_id: 2,
            placement: ViewerTabPlacement::Before as i32,
        })
        .await
        .unwrap_err();

    assert_eq!(error.code(), tonic::Code::NotFound);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn settings_service_notifies_the_viewer_after_a_theme_change() -> TestResult {
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("config.toml");
    let server = ServerHarness::start(directory.path(), Some(settings_path)).await?;
    let mut viewer = ViewerServiceClient::new(server.native_channel());
    let mut viewer_updates = viewer
        .watch_viewer(WatchViewerRequest::default())
        .await?
        .into_inner();
    let initial_version = viewer_updates
        .next()
        .await
        .ok_or("viewer watch ended before its initial version")??
        .version;
    let mut settings = SettingsServiceClient::new(server.native_channel());

    settings
        .set_viewer_theme(SetViewerThemeRequest {
            theme: ViewerTheme::Glacier as i32,
        })
        .await?;

    let changed_version = tokio::time::timeout(Duration::from_secs(1), viewer_updates.next())
        .await
        .map_err(|_| "viewer was not notified about the theme change")?
        .ok_or("viewer watch ended before the theme change")??
        .version;
    assert!(changed_version > initial_version);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn viewer_edit_settings_preserves_false_updates_over_a_real_listener() -> TestResult {
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("config.toml");
    let server = ServerHarness::start(directory.path(), Some(settings_path)).await?;
    let mut viewer = ViewerServiceClient::new(server.native_channel());

    viewer
        .edit_settings(EditSettingsRequest {
            wrap_lines: Some(BoolFieldUpdate {
                operation: Some(bool_field_update::Operation::Update(true)),
            }),
            projects_sort: Some(gtl_wire::v1::ProjectsSortFieldUpdate {
                operation: Some(gtl_wire::v1::projects_sort_field_update::Operation::Update(
                    gtl_wire::v1::ProjectsSort::Name as i32,
                )),
            }),
            projects_page_size: Some(gtl_wire::v1::ProjectsPageSizeFieldUpdate {
                operation: Some(
                    gtl_wire::v1::projects_page_size_field_update::Operation::Update(30),
                ),
            }),
            push_confirmation_required: Some(BoolFieldUpdate {
                operation: Some(bool_field_update::Operation::Update(false)),
            }),
            ..Default::default()
        })
        .await?;
    let settings = viewer
        .get_viewer_settings(GetViewerSettingsRequest {})
        .await?
        .into_inner();
    assert_eq!(settings.projects_page_size, 30);
    assert_eq!(
        settings.projects_sort,
        gtl_wire::v1::ProjectsSort::Name as i32
    );
    assert!(
        settings
            .render_options
            .ok_or("missing render options")?
            .wrap_lines
    );
    assert!(!settings.push_confirmation_required);

    let error = viewer
        .edit_settings(EditSettingsRequest {
            projects_page_size: Some(gtl_wire::v1::ProjectsPageSizeFieldUpdate {
                operation: Some(
                    gtl_wire::v1::projects_page_size_field_update::Operation::Update(20),
                ),
            }),
            ..Default::default()
        })
        .await
        .err()
        .ok_or("invalid page size was accepted")?;
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    viewer
        .edit_settings(EditSettingsRequest {
            projects_page_size: Some(gtl_wire::v1::ProjectsPageSizeFieldUpdate {
                operation: Some(
                    gtl_wire::v1::projects_page_size_field_update::Operation::Clear(
                        gtl_wire::v1::ClearSetting {},
                    ),
                ),
            }),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        viewer
            .get_viewer_settings(GetViewerSettingsRequest {})
            .await?
            .into_inner()
            .projects_page_size,
        15
    );

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn viewer_edit_settings_rejects_a_stale_document_revision_over_a_real_listener() -> TestResult
{
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("config.toml");
    let server = ServerHarness::start(directory.path(), Some(settings_path.clone())).await?;
    let mut viewer = ViewerServiceClient::new(server.native_channel());
    let revision = viewer
        .get_viewer_settings(GetViewerSettingsRequest {})
        .await?
        .into_inner()
        .revision;
    let concurrent_contents = "theme = \"mirage\"\n";
    std::fs::write(&settings_path, concurrent_contents)?;

    let error = viewer
        .edit_settings(EditSettingsRequest {
            expected_revision: Some(revision),
            wrap_lines: Some(BoolFieldUpdate {
                operation: Some(bool_field_update::Operation::Update(true)),
            }),
            ..Default::default()
        })
        .await
        .err()
        .ok_or("stale settings revision was accepted")?;

    assert_eq!(error.code(), tonic::Code::Aborted);
    assert_eq!(std::fs::read_to_string(settings_path)?, concurrent_contents);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn shutdown_reports_not_serving_and_stops_with_an_open_health_watch() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut server = ServerHarness::start(directory.path(), None).await?;
    let mut server_health = health_watch(server.native_channel(), "").await?;
    let mut diff_health = health_watch(server.native_channel(), "gtl.v1.DiffService").await?;

    assert_health_update(&mut server_health, ServingStatus::Serving).await?;
    assert_health_update(&mut diff_health, ServingStatus::Serving).await?;
    server.begin_shutdown()?;
    assert_health_update(&mut diff_health, ServingStatus::NotServing).await?;
    assert_health_update(&mut server_health, ServingStatus::NotServing).await?;

    let server_result = tokio::time::timeout(Duration::from_secs(1), server.wait())
        .await
        .map_err(|_| "test server exceeded its shutdown grace period")?;
    server_result?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn writes_transport_traces_without_private_metadata() -> TestResult {
    let trace_directory = tempfile::tempdir()?;
    let log_directory = trace_directory.path().join("logs");
    let (dispatch, observability_guard) = build_test_dispatch(log_directory.clone())?;
    let default_dispatch_guard = tracing::dispatcher::set_default(&dispatch);
    let data_directory = tempfile::tempdir()?;
    let server = ServerHarness::start(data_directory.path(), None).await?;

    let mut diff = DiffServiceClient::new(server.native_channel());
    let mut request = Request::new(relative_working_directory_diff_request());
    request
        .metadata_mut()
        .insert("x-gtl-private-test", PRIVATE_METADATA_VALUE.parse()?);
    let error = diff.render_diff(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    HealthClient::new(server.native_channel())
        .check(HealthCheckRequest {
            service: "gtl.v1.DiffService".to_owned(),
        })
        .await?;
    request_reflection(server.native_channel()).await?;
    server.stop().await?;
    drop(default_dispatch_guard);
    drop(dispatch);
    drop(observability_guard);

    let records = read_json_records(&log_directory)?;
    for uri in [DIFF_RENDER_URI, HEALTH_CHECK_URI, REFLECTION_URI] {
        assert!(
            records.iter().any(|record| record_has_uri(record, uri)),
            "durable traces omitted {uri}"
        );
    }
    for uri in [HEALTH_CHECK_URI, REFLECTION_URI] {
        assert!(
            records.iter().any(|record| record_is_success(record, uri)),
            "durable traces omitted successful status and latency for {uri}"
        );
    }
    assert!(records.iter().any(|record| {
        record_has_uri(record, DIFF_RENDER_URI)
            && record["fields"]["status"] == i64::from(tonic::Code::InvalidArgument as i32)
            && record["fields"].get("latency").is_some()
    }));
    assert!(
        records
            .iter()
            .all(|record| !record.to_string().contains(PRIVATE_METADATA_VALUE))
    );
    Ok(())
}

async fn health_watch(
    channel: Channel,
    service: &str,
) -> TestResult<tonic::Streaming<tonic_health::pb::HealthCheckResponse>> {
    Ok(HealthClient::new(channel)
        .watch(HealthCheckRequest {
            service: service.to_owned(),
        })
        .await?
        .into_inner())
}

async fn assert_health_serving(channel: Channel) -> TestResult {
    let mut client = HealthClient::new(channel);
    for service in [
        "",
        "gtl.v1.DiffService",
        "gtl.v1.ProjectService",
        "gtl.v1.RepositoryService",
        "gtl.v1.SettingsService",
        "gtl.v1.TagService",
        "gtl.v1.ViewerService",
    ] {
        let response = client
            .check(HealthCheckRequest {
                service: service.to_owned(),
            })
            .await?
            .into_inner();
        assert_eq!(response.status, ServingStatus::Serving as i32);
    }
    Ok(())
}

async fn assert_health_update(
    updates: &mut tonic::Streaming<tonic_health::pb::HealthCheckResponse>,
    expected: ServingStatus,
) -> TestResult {
    let response = tokio::time::timeout(Duration::from_secs(1), updates.message())
        .await
        .map_err(|_| "health update timed out")??
        .ok_or("health watch ended before the expected update")?;

    assert_eq!(response.status, expected as i32);
    Ok(())
}

async fn assert_reflection_describes_gtl_contract(channel: Channel) -> TestResult {
    let requests = tokio_stream::iter([
        ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::ListServices(String::new())),
        },
        ServerReflectionRequest {
            host: String::new(),
            message_request: Some(MessageRequest::FileByFilename(
                "gtl/v1/diff.proto".to_owned(),
            )),
        },
    ]);
    let mut responses = ServerReflectionClient::new(channel)
        .server_reflection_info(Request::new(requests))
        .await?
        .into_inner();
    let mut advertised_services = match next_reflection_response(&mut responses).await? {
        Some(MessageResponse::ListServicesResponse(response)) => response
            .service
            .into_iter()
            .map(|service| service.name)
            .collect::<Vec<_>>(),
        _ => return Err("reflection returned an unexpected service-list response".into()),
    };
    advertised_services.sort_unstable();
    assert_eq!(
        advertised_services,
        [
            "grpc.health.v1.Health",
            "grpc.reflection.v1.ServerReflection",
            "gtl.v1.DiffService",
            "gtl.v1.ProjectService",
            "gtl.v1.RepositoryService",
            "gtl.v1.SettingsService",
            "gtl.v1.TagService",
            "gtl.v1.ViewerService",
        ]
    );
    let encoded_descriptors = match next_reflection_response(&mut responses).await? {
        Some(MessageResponse::FileDescriptorResponse(response)) => response.file_descriptor_proto,
        _ => return Err("reflection returned an unexpected descriptor response".into()),
    };
    let descriptors = encoded_descriptors
        .iter()
        .map(|descriptor| FileDescriptorProto::decode(descriptor.as_slice()))
        .collect::<Result<Vec<_>, _>>()?;
    let descriptor = descriptors
        .iter()
        .find(|descriptor| descriptor.name.as_deref() == Some("gtl/v1/diff.proto"))
        .ok_or("reflection omitted the GTL descriptor")?;
    assert_eq!(
        descriptor
            .service
            .iter()
            .filter_map(|service| service.name.as_deref())
            .collect::<Vec<_>>(),
        ["DiffService"]
    );
    Ok(())
}

async fn request_reflection(channel: Channel) -> TestResult {
    let requests = tokio_stream::iter([ServerReflectionRequest {
        host: String::new(),
        message_request: Some(MessageRequest::ListServices(String::new())),
    }]);
    let mut responses = ServerReflectionClient::new(channel)
        .server_reflection_info(Request::new(requests))
        .await?
        .into_inner();
    while responses.message().await?.is_some() {}
    Ok(())
}

fn record_has_uri(record: &Value, uri: &str) -> bool {
    span_has_uri(&record["span"], uri)
        || record["spans"]
            .as_array()
            .is_some_and(|spans| spans.iter().any(|span| span_has_uri(span, uri)))
}

fn span_has_uri(span: &Value, uri: &str) -> bool {
    span["uri"]
        .as_str()
        .is_some_and(|recorded_uri| recorded_uri.ends_with(uri))
}

fn record_is_success(record: &Value, uri: &str) -> bool {
    let fields = &record["fields"];
    record_has_uri(record, uri)
        && fields["status"] == 0
        && (fields.get("latency").is_some() || fields.get("stream_duration").is_some())
}

fn relative_working_directory_diff_request() -> RenderDiffRequest {
    RenderDiffRequest {
        working_directory: "relative".into(),
        target: Some(DiffTarget {
            selection: Some(diff_target::Selection::Unpushed(Empty {})),
        }),
        name: None,
    }
}

async fn next_reflection_response(
    responses: &mut tonic::Streaming<ServerReflectionResponse>,
) -> TestResult<Option<MessageResponse>> {
    Ok(responses
        .next()
        .await
        .ok_or("reflection stream returned no response")??
        .message_response)
}

#[tokio::test]
#[serial(server_tracing)]
async fn rejects_relative_project_paths_before_catalogue_access() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = ViewerServiceClient::new(server.native_channel());
    let error = client
        .open_viewer_project(gtl_wire::v1::OpenViewerProjectRequest {
            path: "relative".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn snapshot_rename_validates_requests_through_the_generated_client() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = ViewerServiceClient::new(server.native_channel());
    for (tab_id, name, code) in [
        (0, "Review", tonic::Code::InvalidArgument),
        (1, "", tonic::Code::InvalidArgument),
        (1, "Review", tonic::Code::NotFound),
    ] {
        let error = client
            .rename_viewer_snapshot(gtl_wire::v1::RenameViewerSnapshotRequest {
                tab_id,
                name: name.into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), code);
    }
    server.stop().await?;
    Ok(())
}
