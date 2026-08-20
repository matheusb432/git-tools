use std::{error::Error, time::Duration};

use gtl_local_auth::CapabilityToken;
use gtl_wire::v1::{
    DiffTarget, Empty, GetRecursiveRepositoryStatusesRequest, GetRepositoryStatusRequest,
    RenderDiffRequest, diff_service_client::DiffServiceClient, diff_target,
    repository_service_client::RepositoryServiceClient,
};
use prost::Message as _;
use prost_types::FileDescriptorProto;
use tokio::{sync::oneshot, task::JoinHandle};
use tokio_stream::StreamExt as _;
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::Channel,
};
use tonic_health::{
    ServingStatus,
    pb::{HealthCheckRequest, health_client::HealthClient},
};
use tonic_reflection::pb::v1::{
    ServerReflectionRequest, ServerReflectionResponse,
    server_reflection_client::ServerReflectionClient, server_reflection_request::MessageRequest,
    server_reflection_response::MessageResponse,
};

use super::serve;
use crate::state::AppState;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

const TEST_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(250);

struct TestServer {
    directory: tempfile::TempDir,
    channel: Channel,
    authorization: TestAuthorization,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<anyhow::Result<()>>,
}

impl TestServer {
    async fn start() -> TestResult<Self> {
        let directory = tempfile::tempdir()?;
        let state = AppState::open(directory.path())?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let capability = CapabilityToken::generate()?;
        let authorization = TestAuthorization::new(&capability)?;
        let (shutdown, shutdown_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            serve(
                listener,
                async move {
                    let _ = shutdown_receiver.await;
                },
                TEST_SHUTDOWN_GRACE_PERIOD,
                capability,
                state,
            )
            .await
        });
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{address}"))?
            .connect()
            .await?;

        Ok(Self {
            directory,
            channel,
            authorization,
            shutdown,
            task,
        })
    }

    async fn stop(self) -> TestResult {
        self.shutdown
            .send(())
            .map_err(|()| "test server stopped before shutdown")?;
        self.task.await??;
        Ok(())
    }
}

#[derive(Clone)]
struct TestAuthorization {
    value: MetadataValue<Ascii>,
}

impl TestAuthorization {
    fn new(capability: &CapabilityToken) -> TestResult<Self> {
        Ok(Self {
            value: format!("Bearer {}", capability.expose_secret()).parse()?,
        })
    }
}

impl Interceptor for TestAuthorization {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        request
            .metadata_mut()
            .insert("authorization", self.value.clone());
        Ok(request)
    }
}

#[tokio::test]
async fn serves_authenticated_health_and_reflection() -> TestResult {
    let server = TestServer::start().await?;

    assert_health_serving(server.channel.clone(), server.authorization.clone()).await?;
    assert_reflection_describes_gtl_contract(server.channel.clone(), server.authorization.clone())
        .await?;

    server.stop().await
}

#[tokio::test]
async fn validates_application_requests_through_the_generated_client() -> TestResult {
    let server = TestServer::start().await?;
    let mut client =
        DiffServiceClient::with_interceptor(server.channel.clone(), server.authorization.clone());

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
    server.stop().await
}

#[tokio::test]
async fn maps_repository_discovery_failures_to_grpc_statuses() -> TestResult {
    let server = TestServer::start().await?;
    let mut client = RepositoryServiceClient::with_interceptor(
        server.channel.clone(),
        server.authorization.clone(),
    );
    let root = server.directory.path().to_string_lossy().into_owned();

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

    server.stop().await
}

#[tokio::test]
async fn rejects_requests_without_the_capability() -> TestResult {
    let server = TestServer::start().await?;

    let error = HealthClient::new(server.channel.clone())
        .check(HealthCheckRequest {
            service: String::new(),
        })
        .await
        .expect_err("unauthenticated request must fail");
    assert_eq!(error.code(), tonic::Code::Unauthenticated);

    server.stop().await
}

#[tokio::test]
async fn shutdown_reports_not_serving_and_stops_with_an_open_health_watch() -> TestResult {
    let server = TestServer::start().await?;
    let mut server_health =
        health_watch(server.channel.clone(), server.authorization.clone(), "").await?;
    let mut diff_health = health_watch(
        server.channel.clone(),
        server.authorization.clone(),
        "gtl.v1.DiffService",
    )
    .await?;

    assert_health_update(&mut server_health, ServingStatus::Serving).await?;
    assert_health_update(&mut diff_health, ServingStatus::Serving).await?;
    server
        .shutdown
        .send(())
        .map_err(|()| "test server stopped before shutdown")?;
    assert_health_update(&mut diff_health, ServingStatus::NotServing).await?;
    assert_health_update(&mut server_health, ServingStatus::NotServing).await?;

    let server_result = tokio::time::timeout(Duration::from_secs(1), server.task)
        .await
        .map_err(|_| "test server exceeded its shutdown grace period")?;
    server_result??;
    Ok(())
}

async fn health_watch(
    channel: Channel,
    authorization: TestAuthorization,
    service: &str,
) -> TestResult<tonic::Streaming<tonic_health::pb::HealthCheckResponse>> {
    Ok(HealthClient::with_interceptor(channel, authorization)
        .watch(HealthCheckRequest {
            service: service.to_owned(),
        })
        .await?
        .into_inner())
}

async fn assert_health_serving(channel: Channel, authorization: TestAuthorization) -> TestResult {
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
    authorization: TestAuthorization,
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
