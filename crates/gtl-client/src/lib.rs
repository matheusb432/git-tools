//! Authenticated local gRPC client for `gtl-server`.

use std::time::Duration;

use gtl_local_auth::{CapabilityToken, LocalAuth, LocalAuthError, ServerEndpoint};
use gtl_wire::v1::{
    self, diff_service_client::DiffServiceClient, live_view_service_client::LiveViewServiceClient,
    project_service_client::ProjectServiceClient,
    repository_service_client::RepositoryServiceClient,
    settings_service_client::SettingsServiceClient, tag_service_client::TagServiceClient,
    worktree_service_client::WorktreeServiceClient,
};
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    service::{Interceptor, interceptor::InterceptedService},
    transport::{Channel, Endpoint},
};
use tonic_health::pb::{HealthCheckRequest, health_client::HealthClient};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const OPERATION_TIMEOUT: Duration = Duration::from_mins(30);
const MAX_REQUEST_MESSAGE_SIZE: usize = 64 * 1024;
const MAX_RESPONSE_MESSAGE_SIZE: usize = 4 * 1024 * 1024;
const AUTHORIZATION_METADATA_KEY: &str = "authorization";

type AuthenticatedChannel = InterceptedService<Channel, RequestPolicy>;

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error(transparent)]
    LocalBootstrap(#[from] LocalAuthError),
    #[error("local gtl-server endpoint is not a valid URI")]
    InvalidEndpoint(#[source] tonic::transport::Error),
    #[error("local capability token cannot be encoded as gRPC metadata")]
    AuthorizationMetadata(#[source] tonic::metadata::errors::InvalidMetadataValue),
    #[error("could not connect to local gtl-server")]
    Transport(#[source] tonic::transport::Error),
    #[error("local gtl-server health check failed")]
    Health(#[source] Status),
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("gtl-server request failed: {0}")]
    Rpc(#[from] Status),
}

#[derive(Debug, Clone)]
pub struct GtlClient {
    channel: Channel,
    request_policy: RequestPolicy,
    endpoint: ServerEndpoint,
}

impl GtlClient {
    /// Discovers and authenticates the OS-managed local server.
    pub async fn connect_local() -> Result<Self, ConnectError> {
        let auth = LocalAuth::from_environment()?;
        Self::connect(&auth).await
    }

    /// Discovers and authenticates the server published in `auth`'s data root.
    pub async fn connect(auth: &LocalAuth) -> Result<Self, ConnectError> {
        let endpoint = auth.load_endpoint()?;
        let token = auth.load_client_token()?;
        let client = Self::connect_endpoint(&endpoint, &token).await?;
        client
            .check_health_inner()
            .await
            .map_err(ConnectError::Health)?;
        Ok(client)
    }

    pub async fn check_health(&self) -> Result<(), ClientError> {
        self.check_health_inner().await.map_err(ClientError::from)
    }

    pub async fn render_diff(
        &self,
        request: v1::RenderDiffRequest,
    ) -> Result<v1::RenderDiffResponse, ClientError> {
        self.diff_client()
            .render(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn prepare_diff(
        &self,
        request: v1::PrepareDiffRequest,
    ) -> Result<v1::PrepareDiffResponse, ClientError> {
        self.diff_client()
            .prepare(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn prepare_merge_diff(
        &self,
        request: v1::PrepareMergeDiffRequest,
    ) -> Result<v1::PrepareDiffResponse, ClientError> {
        self.diff_client()
            .prepare_merge(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn prepare_subrepositories_diff(
        &self,
        request: v1::PrepareSubrepositoriesDiffRequest,
    ) -> Result<v1::PrepareDiffResponse, ClientError> {
        self.diff_client()
            .prepare_subrepositories(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn prepare_projects_diff(
        &self,
        request: v1::PrepareProjectsDiffRequest,
    ) -> Result<v1::PrepareDiffResponse, ClientError> {
        self.diff_client()
            .prepare_projects(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn render_merge_diff(
        &self,
        request: v1::RenderMergeDiffRequest,
    ) -> Result<v1::RenderDiffResponse, ClientError> {
        self.diff_client()
            .render_merge(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn render_subrepositories_diff(
        &self,
        request: v1::RenderSubrepositoriesDiffRequest,
    ) -> Result<v1::RenderDiffResponse, ClientError> {
        self.diff_client()
            .render_subrepositories(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn render_projects_diff(
        &self,
        request: v1::RenderProjectsDiffRequest,
    ) -> Result<v1::RenderDiffResponse, ClientError> {
        self.diff_client()
            .render_projects(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn push_project_repositories(
        &self,
        request: v1::SyncProjectsRequest,
    ) -> Result<v1::SyncProjectsResponse, ClientError> {
        self.project_client()
            .push_repositories(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn pull_project_repositories(
        &self,
        request: v1::SyncProjectsRequest,
    ) -> Result<v1::SyncProjectsResponse, ClientError> {
        self.project_client()
            .pull_repositories(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn commit_project_repositories(
        &self,
        request: v1::CommitProjectRepositoriesRequest,
    ) -> Result<v1::CommitProjectRepositoriesResponse, ClientError> {
        self.project_client()
            .commit_repositories(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn prune_project_branches(
        &self,
        request: v1::PruneProjectBranchesRequest,
    ) -> Result<v1::PruneProjectBranchesResponse, ClientError> {
        self.project_client()
            .prune_branches(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_project_repository_statuses(
        &self,
    ) -> Result<v1::RepositoryStatusesResponse, ClientError> {
        self.project_client()
            .get_statuses(v1::Empty {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_repository_push(
        &self,
        request: v1::PlanRepositoryPushRequest,
    ) -> Result<v1::PlanRepositoryPushResponse, ClientError> {
        self.repository_client()
            .plan_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_repository_push(
        &self,
        request: v1::ExecuteRepositoryPushRequest,
    ) -> Result<v1::ExecuteRepositoryPushResponse, ClientError> {
        self.repository_client()
            .execute_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_repository_commit(
        &self,
        request: v1::PlanRepositoryCommitRequest,
    ) -> Result<v1::PlanRepositoryCommitResponse, ClientError> {
        self.repository_client()
            .plan_commit(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_repository_commit(
        &self,
        request: v1::ExecuteRepositoryCommitRequest,
    ) -> Result<v1::ExecuteRepositoryCommitResponse, ClientError> {
        self.repository_client()
            .execute_commit(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_recursive_push(
        &self,
        request: v1::PlanRecursivePushRequest,
    ) -> Result<v1::PlanRecursivePushResponse, ClientError> {
        self.repository_client()
            .plan_recursive_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_recursive_push(
        &self,
        request: v1::ExecuteRecursivePushRequest,
    ) -> Result<v1::ExecuteRecursivePushResponse, ClientError> {
        self.repository_client()
            .execute_recursive_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn change_repository_branch(
        &self,
        request: v1::ChangeRepositoryBranchRequest,
    ) -> Result<v1::ChangeRepositoryBranchResponse, ClientError> {
        self.repository_client()
            .change_branch(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_repository_prune(
        &self,
        request: v1::PlanRepositoryPruneRequest,
    ) -> Result<v1::PlanRepositoryPruneResponse, ClientError> {
        self.repository_client()
            .plan_prune(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_repository_prune(
        &self,
        request: v1::ExecuteRepositoryPruneRequest,
    ) -> Result<v1::ExecuteRepositoryPruneResponse, ClientError> {
        self.repository_client()
            .execute_prune(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_repository_status(
        &self,
        request: v1::GetRepositoryStatusRequest,
    ) -> Result<v1::RepositoryStatusesResponse, ClientError> {
        self.repository_client()
            .get_status(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_recursive_repository_statuses(
        &self,
        request: v1::GetRecursiveRepositoryStatusesRequest,
    ) -> Result<v1::RepositoryStatusesResponse, ClientError> {
        self.repository_client()
            .get_recursive_statuses(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_settings(&self) -> Result<v1::GetSettingsResponse, ClientError> {
        self.settings_client()
            .get_settings(v1::Empty {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn set_theme(
        &self,
        request: v1::SetThemeRequest,
    ) -> Result<v1::SetThemeResponse, ClientError> {
        self.settings_client()
            .set_theme(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_worktree_base(
        &self,
        request: v1::GetWorktreeBaseRequest,
    ) -> Result<v1::GetWorktreeBaseResponse, ClientError> {
        self.worktree_client()
            .get_base(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn list_worktrees(
        &self,
        request: v1::ListWorktreesRequest,
    ) -> Result<v1::ListWorktreesResponse, ClientError> {
        self.worktree_client()
            .list(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn save_live_view(
        &self,
        request: v1::SaveLiveViewRequest,
    ) -> Result<v1::SaveLiveViewResponse, ClientError> {
        self.live_view_client()
            .save(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn save_project_live_views(
        &self,
    ) -> Result<v1::SaveProjectLiveViewsResponse, ClientError> {
        self.live_view_client()
            .save_projects(v1::Empty {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_tag_bump(
        &self,
        request: v1::PlanTagBumpRequest,
    ) -> Result<v1::PlanTagBumpResponse, ClientError> {
        self.tag_client()
            .plan_bump(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_tag_bump(
        &self,
        request: v1::ExecuteTagBumpRequest,
    ) -> Result<v1::ExecuteTagBumpResponse, ClientError> {
        self.tag_client()
            .execute_bump(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn list_tags(
        &self,
        request: v1::ListTagsRequest,
    ) -> Result<v1::ListTagsResponse, ClientError> {
        self.tag_client()
            .list(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn add_tag(
        &self,
        request: v1::AddTagRequest,
    ) -> Result<v1::TagActionResponse, ClientError> {
        self.tag_client()
            .add(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn push_tags(
        &self,
        request: v1::PushTagsRequest,
    ) -> Result<v1::TagActionResponse, ClientError> {
        self.tag_client()
            .push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn add_and_push_tag(
        &self,
        request: v1::AddAndPushTagRequest,
    ) -> Result<v1::TagActionResponse, ClientError> {
        self.tag_client()
            .add_and_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn label_tag(
        &self,
        request: v1::LabelTagRequest,
    ) -> Result<v1::TagActionResponse, ClientError> {
        self.tag_client()
            .label(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    #[must_use]
    pub const fn endpoint(&self) -> &ServerEndpoint {
        &self.endpoint
    }

    fn diff_client(&self) -> DiffServiceClient<AuthenticatedChannel> {
        DiffServiceClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn live_view_client(&self) -> LiveViewServiceClient<AuthenticatedChannel> {
        LiveViewServiceClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn project_client(&self) -> ProjectServiceClient<AuthenticatedChannel> {
        ProjectServiceClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn repository_client(&self) -> RepositoryServiceClient<AuthenticatedChannel> {
        RepositoryServiceClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn settings_client(&self) -> SettingsServiceClient<AuthenticatedChannel> {
        SettingsServiceClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn tag_client(&self) -> TagServiceClient<AuthenticatedChannel> {
        TagServiceClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn worktree_client(&self) -> WorktreeServiceClient<AuthenticatedChannel> {
        WorktreeServiceClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn health_client(&self) -> HealthClient<AuthenticatedChannel> {
        HealthClient::with_interceptor(self.channel.clone(), self.request_policy.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    async fn connect_endpoint(
        endpoint: &ServerEndpoint,
        token: &CapabilityToken,
    ) -> Result<Self, ConnectError> {
        let channel_endpoint = Endpoint::from_shared(format!("http://{}", endpoint.address()))
            .map_err(ConnectError::InvalidEndpoint)?
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(OPERATION_TIMEOUT);
        let channel = channel_endpoint
            .connect()
            .await
            .map_err(ConnectError::Transport)?;
        let request_policy = RequestPolicy::try_new(token)?;
        Ok(Self {
            channel,
            request_policy,
            endpoint: endpoint.clone(),
        })
    }

    async fn check_health_inner(&self) -> Result<(), Status> {
        let mut client = self.health_client();
        let mut request = Request::new(HealthCheckRequest {
            service: String::new(),
        });
        request.set_timeout(HEALTH_TIMEOUT);
        let response = client.check(request).await?.into_inner();
        if response.status != tonic_health::ServingStatus::Serving as i32 {
            return Err(Status::unavailable("gtl-server is not serving"));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct RequestPolicy {
    value: MetadataValue<Ascii>,
}

impl std::fmt::Debug for RequestPolicy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RequestPolicy(REDACTED)")
    }
}

impl RequestPolicy {
    fn try_new(token: &CapabilityToken) -> Result<Self, ConnectError> {
        let value = format!("Bearer {}", token.expose_secret())
            .parse()
            .map_err(ConnectError::AuthorizationMetadata)?;
        Ok(Self { value })
    }
}

impl Interceptor for RequestPolicy {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        request
            .metadata_mut()
            .insert(AUTHORIZATION_METADATA_KEY, self.value.clone());
        if request.metadata().get("grpc-timeout").is_none() {
            request.set_timeout(OPERATION_TIMEOUT);
        }
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, time::Duration};

    use gtl_local_auth::{ServerEndpoint, ServerInstanceId};
    use gtl_wire::v1::{
        DiffTarget, Empty, PrepareDiffRequest, PrepareDiffResponse, PrepareMergeDiffRequest,
        PrepareProjectsDiffRequest, PrepareSubrepositoriesDiffRequest, RenderDiffRequest,
        RenderDiffResponse, RenderMergeDiffRequest, RenderProjectsDiffRequest,
        RenderSubrepositoriesDiffRequest,
        diff_service_server::{DiffService, DiffServiceServer},
        diff_target, render_diff_response,
    };
    use tokio::sync::oneshot;
    use tonic::{
        Response,
        service::InterceptorLayer,
        transport::{Server, server::TcpIncoming},
    };

    use super::*;

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    #[test]
    fn request_policy_adds_authentication_and_a_default_deadline() -> TestResult {
        let capability = CapabilityToken::generate()?;
        let mut policy = RequestPolicy::try_new(&capability)?;
        let request = policy.call(Request::new(()))?;

        let candidate = request
            .metadata()
            .get(AUTHORIZATION_METADATA_KEY)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .ok_or("request policy omitted bearer authentication")?;
        assert!(capability.authenticates(candidate));

        let mut expected = Request::new(());
        expected.set_timeout(OPERATION_TIMEOUT);
        assert_eq!(
            request.metadata().get("grpc-timeout"),
            expected.metadata().get("grpc-timeout")
        );

        let mut health_request = Request::new(());
        health_request.set_timeout(HEALTH_TIMEOUT);
        let expected_health_timeout = health_request
            .metadata()
            .get("grpc-timeout")
            .cloned()
            .ok_or("health request omitted its explicit deadline")?;
        let health_request = policy.call(health_request)?;
        assert_eq!(
            health_request.metadata().get("grpc-timeout"),
            Some(&expected_health_timeout)
        );
        Ok(())
    }

    #[tokio::test]
    async fn connects_through_private_discovery_and_authentication() -> TestResult {
        let harness = TestHarness::start(true).await?;

        let client = GtlClient::connect(&harness.auth).await?;

        client.check_health().await?;
        assert_eq!(client.endpoint().address(), harness.address);
        harness.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_server_with_a_different_capability() -> TestResult {
        let harness = TestHarness::start(false).await?;

        let error = GtlClient::connect(&harness.auth)
            .await
            .expect_err("mismatched capability must fail");

        assert!(matches!(
            error,
            ConnectError::Health(status) if status.code() == tonic::Code::Unauthenticated
        ));
        harness.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn sends_an_authenticated_application_request() -> TestResult {
        let harness = TestHarness::start(true).await?;
        let client = GtlClient::connect(&harness.auth).await?;

        let response = client
            .render_diff(RenderDiffRequest {
                working_directory: "/repo".into(),
                target: Some(DiffTarget {
                    selection: Some(diff_target::Selection::Unpushed(Empty {})),
                }),
                name: None,
            })
            .await?;

        assert!(matches!(
            response.outcome,
            Some(render_diff_response::Outcome::Empty(_))
        ));
        harness.stop().await?;
        Ok(())
    }

    #[derive(Clone)]
    struct TestDiff;

    #[tonic::async_trait]
    impl DiffService for TestDiff {
        async fn prepare(
            &self,
            _request: Request<PrepareDiffRequest>,
        ) -> Result<Response<PrepareDiffResponse>, Status> {
            Ok(Response::new(PrepareDiffResponse { batch: None }))
        }

        async fn prepare_merge(
            &self,
            _request: Request<PrepareMergeDiffRequest>,
        ) -> Result<Response<PrepareDiffResponse>, Status> {
            Ok(Response::new(PrepareDiffResponse { batch: None }))
        }

        async fn prepare_subrepositories(
            &self,
            _request: Request<PrepareSubrepositoriesDiffRequest>,
        ) -> Result<Response<PrepareDiffResponse>, Status> {
            Ok(Response::new(PrepareDiffResponse { batch: None }))
        }

        async fn prepare_projects(
            &self,
            _request: Request<PrepareProjectsDiffRequest>,
        ) -> Result<Response<PrepareDiffResponse>, Status> {
            Ok(Response::new(PrepareDiffResponse { batch: None }))
        }

        async fn render(
            &self,
            _request: Request<RenderDiffRequest>,
        ) -> Result<Response<RenderDiffResponse>, Status> {
            Ok(Response::new(empty_diff_response()))
        }

        async fn render_merge(
            &self,
            _request: Request<RenderMergeDiffRequest>,
        ) -> Result<Response<RenderDiffResponse>, Status> {
            Ok(Response::new(empty_diff_response()))
        }

        async fn render_subrepositories(
            &self,
            _request: Request<RenderSubrepositoriesDiffRequest>,
        ) -> Result<Response<RenderDiffResponse>, Status> {
            Ok(Response::new(empty_diff_response()))
        }

        async fn render_projects(
            &self,
            _request: Request<RenderProjectsDiffRequest>,
        ) -> Result<Response<RenderDiffResponse>, Status> {
            Ok(Response::new(empty_diff_response()))
        }
    }

    fn empty_diff_response() -> RenderDiffResponse {
        RenderDiffResponse {
            notes: Vec::new(),
            outcome: Some(render_diff_response::Outcome::Empty(Empty {})),
        }
    }

    struct TestHarness {
        _directory: tempfile::TempDir,
        _published_endpoint: gtl_local_auth::PublishedEndpoint,
        auth: LocalAuth,
        address: std::net::SocketAddr,
        shutdown: oneshot::Sender<()>,
        task: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    }

    #[derive(Clone)]
    struct TestAuthentication {
        capability: CapabilityToken,
    }

    impl Interceptor for TestAuthentication {
        fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
            let authenticated = request
                .metadata()
                .get(AUTHORIZATION_METADATA_KEY)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .is_some_and(|candidate| self.capability.authenticates(candidate));
            if authenticated {
                Ok(request)
            } else {
                Err(Status::unauthenticated("authentication required"))
            }
        }
    }

    impl TestHarness {
        async fn start(matching_capability: bool) -> TestResult<Self> {
            let directory = tempfile::tempdir()?;
            let auth = LocalAuth::from_data_root(directory.path())?;
            let stored_capability = auth.load_or_create_server_token()?;
            let server_capability = if matching_capability {
                stored_capability
            } else {
                CapabilityToken::generate()?
            };
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let endpoint = ServerEndpoint::try_new(address, ServerInstanceId::generate())?;
            let published_endpoint = auth.publish_endpoint(endpoint)?;
            let authentication = TestAuthentication {
                capability: server_capability,
            };
            let (health_reporter, health_service) = tonic_health::server::health_reporter();
            health_reporter
                .set_service_status("", tonic_health::ServingStatus::Serving)
                .await;
            let (shutdown, shutdown_receiver) = oneshot::channel();
            let task = tokio::spawn(async move {
                Server::builder()
                    .layer(InterceptorLayer::new(authentication))
                    .add_service(health_service)
                    .add_service(DiffServiceServer::new(TestDiff))
                    .serve_with_incoming_shutdown(TcpIncoming::from(listener), async move {
                        let _ = shutdown_receiver.await;
                    })
                    .await
            });

            Ok(Self {
                _directory: directory,
                _published_endpoint: published_endpoint,
                auth,
                address,
                shutdown,
                task,
            })
        }

        async fn stop(self) -> TestResult {
            self.shutdown
                .send(())
                .map_err(|()| "test server stopped before shutdown")?;
            tokio::time::timeout(Duration::from_secs(1), self.task).await???;
            Ok(())
        }
    }
}
