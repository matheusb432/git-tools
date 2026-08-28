use std::{
    error::Error,
    io::{Read as _, Write as _},
    time::Duration,
};

use gtl_wire::v1::{
    DiffTarget, Empty, GetRecursiveRepositoryStatusesRequest, GetRepositoryStatusRequest,
    GetViewerShellRequest, GetWorktreeBaseRequest, PushProjectRepositoriesRequest,
    RenderDiffRequest, SetViewerThemeRequest, ViewerTheme, WatchViewerRequest,
    diff_service_client::DiffServiceClient, diff_target,
    project_service_client::ProjectServiceClient,
    repository_service_client::RepositoryServiceClient,
    settings_service_client::SettingsServiceClient, viewer_service_client::ViewerServiceClient,
    worktree_service_client::WorktreeServiceClient,
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
    harness::{ServerHarness, ServerHarnessAuthorization},
    observability::{build_test_dispatch, read_json_records},
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const PRIVATE_METADATA_VALUE: &str = "gtl-observability-private-metadata";
const DIFF_RENDER_URI: &str = "/gtl.v1.DiffService/RenderDiff";
const HEALTH_CHECK_URI: &str = "/grpc.health.v1.Health/Check";
const REFLECTION_URI: &str = "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo";

#[tokio::test]
#[serial(server_tracing)]
async fn serves_authenticated_health_and_reflection() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;

    assert_health_serving(server.native_channel(), server.authorization()).await?;
    assert_reflection_describes_gtl_contract(server.native_channel(), server.authorization())
        .await?;

    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn validates_application_requests_through_the_generated_client() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client =
        DiffServiceClient::with_interceptor(server.native_channel(), server.authorization());

    let error = client
        .render_diff(relative_working_directory_diff_request())
        .await
        .expect_err("relative working directory must fail");

    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn rejects_map_based_diff_settings_before_managed_push_dependencies() -> TestResult {
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("config.toml");
    std::fs::write(&settings_path, "[diff.exclude]\ndefaults = [\"md\"]\n")?;
    let server = ServerHarness::start(directory.path(), Some(settings_path)).await?;
    let mut client =
        ProjectServiceClient::with_interceptor(server.native_channel(), server.authorization());

    let error = client
        .push_project_repositories(PushProjectRepositoriesRequest { dry_run: true })
        .await
        .expect_err("unsupported settings shape must fail");

    assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    assert_eq!(error.message(), "user settings are invalid");
    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn maps_repository_discovery_failures_to_grpc_statuses() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client =
        RepositoryServiceClient::with_interceptor(server.native_channel(), server.authorization());
    let root = directory.path().to_string_lossy().into_owned();

    let error = client
        .get_repository_status(GetRepositoryStatusRequest {
            repository_path: root.clone(),
        })
        .await
        .expect_err("a plain directory is not a repository");
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);

    let error = client
        .get_recursive_repository_statuses(GetRecursiveRepositoryStatusesRequest { root })
        .await
        .expect_err("an empty traversal has no repositories");
    assert_eq!(error.code(), tonic::Code::NotFound);

    let mut worktree_client =
        WorktreeServiceClient::with_interceptor(server.native_channel(), server.authorization());
    let error = worktree_client
        .get_worktree_base(GetWorktreeBaseRequest {
            repository_path: directory.path().to_string_lossy().into_owned(),
        })
        .await
        .expect_err("worktree lookup requires a repository");
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn rejects_requests_without_the_capability() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;

    let error = HealthClient::new(server.native_channel())
        .check(HealthCheckRequest {
            service: String::new(),
        })
        .await
        .expect_err("unauthenticated request must fail");
    assert_eq!(error.code(), tonic::Code::Unauthenticated);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn viewer_service_accepts_only_the_browser_capability() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;

    let error =
        ViewerServiceClient::with_interceptor(server.viewer_channel(), server.authorization())
            .get_viewer_shell(GetViewerShellRequest {})
            .await
            .expect_err("the native client capability must not authorize the viewer service");
    assert_eq!(error.code(), tonic::Code::Unauthenticated);

    ViewerServiceClient::with_interceptor(server.viewer_channel(), server.viewer_authorization())
        .get_viewer_shell(GetViewerShellRequest {})
        .await?;

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn settings_service_notifies_the_viewer_after_a_theme_change() -> TestResult {
    let directory = tempfile::tempdir()?;
    let settings_path = directory.path().join("config.toml");
    let server = ServerHarness::start(directory.path(), Some(settings_path)).await?;
    let mut viewer = ViewerServiceClient::with_interceptor(
        server.viewer_channel(),
        server.viewer_authorization(),
    );
    let mut viewer_updates = viewer
        .watch_viewer(WatchViewerRequest {})
        .await?
        .into_inner();
    let initial_version = viewer_updates
        .next()
        .await
        .ok_or("viewer watch ended before its initial version")??
        .version;
    let mut settings =
        SettingsServiceClient::with_interceptor(server.native_channel(), server.authorization());

    settings
        .set_viewer_theme(SetViewerThemeRequest {
            theme: ViewerTheme::Light as i32,
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
async fn viewer_cors_accepts_only_the_packaged_and_development_origins() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let address = server.viewer_address();

    for origin in [
        "http://tauri.localhost",
        "tauri://localhost",
        "http://127.0.0.1:8080",
    ] {
        let response = send_http1_request(address, cors_preflight(address, origin)).await?;
        assert!(response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.1 204"));
        let response = response.to_ascii_lowercase();
        assert!(response.contains(&format!("access-control-allow-origin: {origin}")));
        assert!(response.contains("access-control-allow-methods: post"));
        assert!(response.contains("authorization"));
        assert!(response.contains("x-grpc-web"));
    }

    let rejected = send_http1_request(address, cors_preflight(address, "https://example.invalid"))
        .await?
        .to_ascii_lowercase();
    assert!(!rejected.contains("access-control-allow-origin"));

    server.stop().await?;
    Ok(())
}

#[tokio::test]
#[serial(server_tracing)]
async fn shutdown_reports_not_serving_and_stops_with_an_open_health_watch() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut server = ServerHarness::start(directory.path(), None).await?;
    let mut server_health =
        health_watch(server.native_channel(), server.authorization(), "").await?;
    let mut diff_health = health_watch(
        server.native_channel(),
        server.authorization(),
        "gtl.v1.DiffService",
    )
    .await?;

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

    let mut diff =
        DiffServiceClient::with_interceptor(server.native_channel(), server.authorization());
    let mut request = Request::new(relative_working_directory_diff_request());
    request
        .metadata_mut()
        .insert("x-gtl-private-test", PRIVATE_METADATA_VALUE.parse()?);
    let error = diff
        .render_diff(request)
        .await
        .expect_err("relative working directory must fail");
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    HealthClient::with_interceptor(server.native_channel(), server.authorization())
        .check(HealthCheckRequest {
            service: "gtl.v1.DiffService".to_owned(),
        })
        .await?;
    request_reflection(server.native_channel(), server.authorization()).await?;
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
    authorization: ServerHarnessAuthorization,
    service: &str,
) -> TestResult<tonic::Streaming<tonic_health::pb::HealthCheckResponse>> {
    Ok(HealthClient::with_interceptor(channel, authorization)
        .watch(HealthCheckRequest {
            service: service.to_owned(),
        })
        .await?
        .into_inner())
}

async fn assert_health_serving(
    channel: Channel,
    authorization: ServerHarnessAuthorization,
) -> TestResult {
    let mut client = HealthClient::with_interceptor(channel, authorization);
    for service in [
        "",
        "gtl.v1.DiffService",
        "gtl.v1.LiveViewService",
        "gtl.v1.ProjectService",
        "gtl.v1.RepositoryService",
        "gtl.v1.SettingsService",
        "gtl.v1.TagService",
        "gtl.v1.WorktreeService",
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

fn cors_preflight(address: std::net::SocketAddr, origin: &str) -> String {
    format!(
        "OPTIONS /gtl.v1.ViewerService/GetViewerShell HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: authorization,x-grpc-web\r\nConnection: close\r\n\r\n"
    )
}

async fn send_http1_request(address: std::net::SocketAddr, request: String) -> TestResult<String> {
    Ok(
        tokio::task::spawn_blocking(move || -> std::io::Result<String> {
            let mut connection = std::net::TcpStream::connect(address)?;
            connection.set_read_timeout(Some(Duration::from_secs(2)))?;
            connection.write_all(request.as_bytes())?;
            let mut response = String::new();
            connection.read_to_string(&mut response)?;
            Ok(response)
        })
        .await??,
    )
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

async fn assert_reflection_describes_gtl_contract(
    channel: Channel,
    authorization: ServerHarnessAuthorization,
) -> TestResult {
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
    let mut responses = ServerReflectionClient::with_interceptor(channel, authorization)
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
            "gtl.v1.LiveViewService",
            "gtl.v1.ProjectService",
            "gtl.v1.RepositoryService",
            "gtl.v1.SettingsService",
            "gtl.v1.TagService",
            "gtl.v1.WorktreeService",
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

async fn request_reflection(
    channel: Channel,
    authorization: ServerHarnessAuthorization,
) -> TestResult {
    let requests = tokio_stream::iter([ServerReflectionRequest {
        host: String::new(),
        message_request: Some(MessageRequest::ListServices(String::new())),
    }]);
    let mut responses = ServerReflectionClient::with_interceptor(channel, authorization)
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
