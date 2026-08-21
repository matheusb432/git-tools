use std::{error::Error, time::Duration};

use gtl_wire::v1::{
    DiffTarget, Empty, GetRecursiveRepositoryStatusesRequest, GetRepositoryStatusRequest,
    GetWorktreeBaseRequest, RenderDiffRequest, diff_service_client::DiffServiceClient, diff_target,
    repository_service_client::RepositoryServiceClient,
    worktree_service_client::WorktreeServiceClient,
};
use prost::Message as _;
use prost_types::FileDescriptorProto;
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

use crate::harness::{ServerHarness, ServerHarnessAuthorization};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[tokio::test]
async fn serves_authenticated_health_and_reflection() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;

    assert_health_serving(server.channel(), server.authorization()).await?;
    assert_reflection_describes_gtl_contract(server.channel(), server.authorization()).await?;

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn validates_application_requests_through_the_generated_client() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client = DiffServiceClient::with_interceptor(server.channel(), server.authorization());

    let error = client
        .render(RenderDiffRequest {
            working_directory: "relative".into(),
            target: Some(DiffTarget {
                selection: Some(diff_target::Selection::Unpushed(Empty {})),
            }),
            name: None,
        })
        .await
        .expect_err("relative working directory must fail");

    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn maps_repository_discovery_failures_to_grpc_statuses() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;
    let mut client =
        RepositoryServiceClient::with_interceptor(server.channel(), server.authorization());
    let root = directory.path().to_string_lossy().into_owned();

    let error = client
        .get_status(GetRepositoryStatusRequest {
            repository_path: root.clone(),
        })
        .await
        .expect_err("a plain directory is not a repository");
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);

    let error = client
        .get_recursive_statuses(GetRecursiveRepositoryStatusesRequest { root })
        .await
        .expect_err("an empty traversal has no repositories");
    assert_eq!(error.code(), tonic::Code::NotFound);

    let mut worktree_client =
        WorktreeServiceClient::with_interceptor(server.channel(), server.authorization());
    let error = worktree_client
        .get_base(GetWorktreeBaseRequest {
            repository_path: directory.path().to_string_lossy().into_owned(),
        })
        .await
        .expect_err("worktree lookup requires a repository");
    assert_eq!(error.code(), tonic::Code::FailedPrecondition);

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn rejects_requests_without_the_capability() -> TestResult {
    let directory = tempfile::tempdir()?;
    let server = ServerHarness::start(directory.path(), None).await?;

    let error = HealthClient::new(server.channel())
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
async fn shutdown_reports_not_serving_and_stops_with_an_open_health_watch() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut server = ServerHarness::start(directory.path(), None).await?;
    let mut server_health = health_watch(server.channel(), server.authorization(), "").await?;
    let mut diff_health = health_watch(
        server.channel(),
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
    let requests = tokio_stream::iter([ServerReflectionRequest {
        host: String::new(),
        message_request: Some(MessageRequest::FileByFilename(
            "gtl/v1/diff.proto".to_owned(),
        )),
    }]);
    let mut responses = ServerReflectionClient::with_interceptor(channel, authorization)
        .server_reflection_info(Request::new(requests))
        .await?
        .into_inner();
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

async fn next_reflection_response(
    responses: &mut tonic::Streaming<ServerReflectionResponse>,
) -> TestResult<Option<MessageResponse>> {
    Ok(responses
        .next()
        .await
        .ok_or("reflection stream returned no response")??
        .message_response)
}
