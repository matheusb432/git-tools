//! Private local gRPC client for `gtl-server`.

#[cfg(not(target_arch = "wasm32"))]
mod request_failure;
#[cfg(not(target_arch = "wasm32"))]
pub mod terminal_diff;
mod viewer;
pub mod window;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use gtl_local_transport::LocalEndpoint;
#[cfg(not(target_arch = "wasm32"))]
use gtl_wire::v1::{
    self, diff_service_client::DiffServiceClient, project_service_client::ProjectServiceClient,
    repository_service_client::RepositoryServiceClient,
    settings_service_client::SettingsServiceClient, tag_service_client::TagServiceClient,
    viewer_service_client::ViewerServiceClient,
};
#[cfg(not(target_arch = "wasm32"))]
pub use request_failure::RequestFailure;
#[cfg(not(target_arch = "wasm32"))]
use tonic::{Request, Status, transport::Channel};
#[cfg(not(target_arch = "wasm32"))]
use tonic_health::pb::{HealthCheckRequest, health_client::HealthClient};
pub use viewer::ViewerClientError;
#[cfg(all(target_arch = "wasm32", feature = "viewer-ipc"))]
pub use viewer::pick_project_folder;
#[cfg(any(not(target_arch = "wasm32"), feature = "viewer-ipc"))]
pub use viewer::{ViewerClient, ViewerRowStream, ViewerVersionStream};

#[cfg(not(target_arch = "wasm32"))]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
#[cfg(not(target_arch = "wasm32"))]
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(target_arch = "wasm32"))]
const OPERATION_TIMEOUT: Duration = Duration::from_mins(30);
#[cfg(not(target_arch = "wasm32"))]
const MAX_REQUEST_MESSAGE_SIZE: usize = 64 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const MAX_RESPONSE_MESSAGE_SIZE: usize = 4 * 1024 * 1024;
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("could not connect to local gtl-server")]
    Transport(#[from] gtl_local_transport::ConnectError),
    #[error("local gtl-server health check failed")]
    Health(#[source] Status),
}

#[cfg(not(target_arch = "wasm32"))]
impl ConnectError {
    /// Returns the gRPC status when the server rejected its health check.
    #[must_use]
    pub const fn status(&self) -> Option<&Status> {
        match self {
            Self::Health(status) => Some(status),
            Self::Transport(_) => None,
        }
    }

    /// Decodes why the connection could not be used.
    #[must_use]
    pub fn failure(&self) -> RequestFailure {
        match self {
            Self::Health(status) => RequestFailure::from_status(status),
            Self::Transport(_) => RequestFailure::Disconnected,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("{}", .0.message())]
    Rpc(#[from] Status),
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, thiserror::Error)]
pub enum ViewerServerInfoError {
    #[error("{}", .0.message())]
    Rpc(#[from] Status),
    #[error("gtl-server returned an invalid instance ID")]
    InvalidInstanceId,
    #[error("gtl-server returned an invalid viewer protocol version")]
    InvalidProtocolVersion,
}

#[cfg(not(target_arch = "wasm32"))]
impl ViewerServerInfoError {
    /// Decodes the server's failure; `None` means the server answered with invalid values.
    #[must_use]
    pub fn failure(&self) -> Option<RequestFailure> {
        match self {
            Self::Rpc(status) => Some(RequestFailure::from_status(status)),
            Self::InvalidInstanceId | Self::InvalidProtocolVersion => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerServerInfo {
    server_instance_id: String,
    protocol_version: u32,
    server_version: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl ViewerServerInfo {
    fn try_from_response(
        response: &v1::GetViewerServerInfoResponse,
    ) -> Result<Self, ViewerServerInfoError> {
        let server_instance_id = uuid::Uuid::parse_str(&response.server_instance_id)
            .map_err(|_| ViewerServerInfoError::InvalidInstanceId)?
            .to_string();
        if response.protocol_version == 0 {
            return Err(ViewerServerInfoError::InvalidProtocolVersion);
        }
        Ok(Self {
            server_instance_id,
            protocol_version: response.protocol_version,
            server_version: response.server_version.clone(),
        })
    }

    #[must_use]
    pub fn server_instance_id(&self) -> &str {
        &self.server_instance_id
    }

    #[must_use]
    pub fn server_version(&self) -> &str {
        &self.server_version
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ClientError {
    /// Returns the gRPC status received for the failed request.
    #[must_use]
    pub const fn status(&self) -> &Status {
        match self {
            Self::Rpc(status) => status,
        }
    }

    /// Decodes the typed failure the server reported, or a lost connection.
    #[must_use]
    pub fn failure(&self) -> RequestFailure {
        RequestFailure::from_status(self.status())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct GtlClient {
    channel: Channel,
    endpoint: LocalEndpoint,
}

#[cfg(not(target_arch = "wasm32"))]
impl GtlClient {
    /// Resolves and connects to the OS-managed local server.
    pub async fn connect_local() -> Result<Self, ConnectError> {
        let endpoint =
            LocalEndpoint::from_environment().map_err(gtl_local_transport::ConnectError::from)?;
        Self::connect(&endpoint).await
    }

    /// Connects to an explicitly resolved local endpoint.
    pub async fn connect(endpoint: &LocalEndpoint) -> Result<Self, ConnectError> {
        let channel = endpoint
            .connect(CONNECT_TIMEOUT, OPERATION_TIMEOUT)
            .await
            .map_err(ConnectError::Transport)?;
        let client = Self {
            channel,
            endpoint: endpoint.clone(),
        };
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
            .render_diff(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn present_diff(
        &self,
        request: v1::PresentDiffRequest,
    ) -> Result<v1::PresentDiffResponse, ClientError> {
        self.diff_client()
            .present_diff(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn present_merge_diff(
        &self,
        request: v1::PresentMergeDiffRequest,
    ) -> Result<v1::PresentMergeDiffResponse, ClientError> {
        self.diff_client()
            .present_merge_diff(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn present_subrepository_diffs(
        &self,
        request: v1::PresentSubrepositoryDiffsRequest,
    ) -> Result<v1::PresentSubrepositoryDiffsResponse, ClientError> {
        self.diff_client()
            .present_subrepository_diffs(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn present_project_repository_diffs(
        &self,
        request: v1::PresentProjectRepositoryDiffsRequest,
    ) -> Result<v1::PresentProjectRepositoryDiffsResponse, ClientError> {
        self.diff_client()
            .present_project_repository_diffs(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn render_merge_diff(
        &self,
        request: v1::RenderMergeDiffRequest,
    ) -> Result<v1::RenderMergeDiffResponse, ClientError> {
        self.diff_client()
            .render_merge_diff(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn render_subrepository_diffs(
        &self,
        request: v1::RenderSubrepositoryDiffsRequest,
    ) -> Result<v1::RenderSubrepositoryDiffsResponse, ClientError> {
        self.diff_client()
            .render_subrepository_diffs(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn render_project_repository_diffs(
        &self,
        request: v1::RenderProjectRepositoryDiffsRequest,
    ) -> Result<v1::RenderProjectRepositoryDiffsResponse, ClientError> {
        self.diff_client()
            .render_project_repository_diffs(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn push_project_repositories(
        &self,
        request: v1::PushProjectRepositoriesRequest,
    ) -> Result<v1::PushProjectRepositoriesResponse, ClientError> {
        self.project_client()
            .push_project_repositories(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn pull_project_repositories(
        &self,
        request: v1::PullProjectRepositoriesRequest,
    ) -> Result<v1::PullProjectRepositoriesResponse, ClientError> {
        self.project_client()
            .pull_project_repositories(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_project_repository_statuses(
        &self,
    ) -> Result<v1::GetProjectRepositoryStatusesResponse, ClientError> {
        self.project_client()
            .get_project_repository_statuses(v1::GetProjectRepositoryStatusesRequest {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_repository_push(
        &self,
        request: v1::PlanRepositoryPushRequest,
    ) -> Result<v1::PlanRepositoryPushResponse, ClientError> {
        self.repository_client()
            .plan_repository_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_repository_push(
        &self,
        request: v1::ExecuteRepositoryPushRequest,
    ) -> Result<v1::ExecuteRepositoryPushResponse, ClientError> {
        self.repository_client()
            .execute_repository_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_recursive_repository_push(
        &self,
        request: v1::PlanRecursiveRepositoryPushRequest,
    ) -> Result<v1::PlanRecursiveRepositoryPushResponse, ClientError> {
        self.repository_client()
            .plan_recursive_repository_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_recursive_repository_push(
        &self,
        request: v1::ExecuteRecursiveRepositoryPushRequest,
    ) -> Result<v1::ExecuteRecursiveRepositoryPushResponse, ClientError> {
        self.repository_client()
            .execute_recursive_repository_push(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_project_repository(
        &self,
        request: v1::GetProjectRepositoryRequest,
    ) -> Result<v1::GetProjectRepositoryResponse, ClientError> {
        self.project_client()
            .get_project_repository(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn pull_repository(
        &self,
        request: v1::PullRepositoryRequest,
    ) -> Result<v1::PullRepositoryResponse, ClientError> {
        self.repository_client()
            .pull_repository(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_repository_status(
        &self,
        request: v1::GetRepositoryStatusRequest,
    ) -> Result<v1::GetRepositoryStatusResponse, ClientError> {
        self.repository_client()
            .get_repository_status(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_recursive_repository_statuses(
        &self,
        request: v1::GetRecursiveRepositoryStatusesRequest,
    ) -> Result<v1::GetRecursiveRepositoryStatusesResponse, ClientError> {
        self.repository_client()
            .get_recursive_repository_statuses(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_push_confirmation_requirement(
        &self,
    ) -> Result<v1::GetPushConfirmationRequirementResponse, ClientError> {
        self.settings_client()
            .get_push_confirmation_requirement(v1::GetPushConfirmationRequirementRequest {})
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn set_viewer_theme(
        &self,
        request: v1::SetViewerThemeRequest,
    ) -> Result<v1::SetViewerThemeResponse, ClientError> {
        self.settings_client()
            .set_viewer_theme(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn plan_tag_bump(
        &self,
        request: v1::PlanTagBumpRequest,
    ) -> Result<v1::PlanTagBumpResponse, ClientError> {
        self.tag_client()
            .plan_tag_bump(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn execute_tag_bump(
        &self,
        request: v1::ExecuteTagBumpRequest,
    ) -> Result<v1::ExecuteTagBumpResponse, ClientError> {
        self.tag_client()
            .execute_tag_bump(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn list_tags(
        &self,
        request: v1::ListTagsRequest,
    ) -> Result<v1::ListTagsResponse, ClientError> {
        self.tag_client()
            .list_tags(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn add_tag(
        &self,
        request: v1::AddTagRequest,
    ) -> Result<v1::AddTagResponse, ClientError> {
        self.tag_client()
            .add_tag(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn push_tags(
        &self,
        request: v1::PushTagsRequest,
    ) -> Result<v1::PushTagsResponse, ClientError> {
        self.tag_client()
            .push_tags(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn add_and_push_tag(
        &self,
        request: v1::AddAndPushTagRequest,
    ) -> Result<v1::AddAndPushTagResponse, ClientError> {
        self.tag_client()
            .add_and_push_tag(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn label_tag(
        &self,
        request: v1::LabelTagRequest,
    ) -> Result<v1::LabelTagResponse, ClientError> {
        self.tag_client()
            .label_tag(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    #[must_use]
    pub const fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }

    pub async fn get_viewer_server_info(&self) -> Result<ViewerServerInfo, ViewerServerInfoError> {
        let mut request = Request::new(v1::GetViewerServerInfoRequest {});
        request.set_timeout(HEALTH_TIMEOUT);
        let response = ViewerServiceClient::new(self.channel.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
            .get_viewer_server_info(request)
            .await?
            .into_inner();
        ViewerServerInfo::try_from_response(&response)
    }

    fn diff_client(&self) -> DiffServiceClient<Channel> {
        DiffServiceClient::new(self.channel.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    pub async fn create_project(
        &self,
        request: v1::CreateProjectRequest,
    ) -> Result<v1::CreateProjectResponse, ClientError> {
        self.project_client()
            .create_project(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn get_project(
        &self,
        request: v1::GetProjectRequest,
    ) -> Result<v1::GetProjectResponse, ClientError> {
        self.project_client()
            .get_project(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn list_active_projects(
        &self,
        request: v1::ListActiveProjectsRequest,
    ) -> Result<v1::ListActiveProjectsResponse, ClientError> {
        self.project_client()
            .list_active_projects(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn pause_project(
        &self,
        request: v1::PauseProjectRequest,
    ) -> Result<v1::PauseProjectResponse, ClientError> {
        self.project_client()
            .pause_project(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn resume_project(
        &self,
        request: v1::ResumeProjectRequest,
    ) -> Result<v1::ResumeProjectResponse, ClientError> {
        self.project_client()
            .resume_project(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn manage_projects(
        &self,
        request: v1::ManageProjectsRequest,
    ) -> Result<v1::ManageProjectsResponse, ClientError> {
        self.project_client()
            .manage_projects(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    pub async fn unmanage_projects(
        &self,
        request: v1::UnmanageProjectsRequest,
    ) -> Result<v1::UnmanageProjectsResponse, ClientError> {
        self.project_client()
            .unmanage_projects(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(ClientError::from)
    }

    fn project_client(&self) -> ProjectServiceClient<Channel> {
        ProjectServiceClient::new(self.channel.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn repository_client(&self) -> RepositoryServiceClient<Channel> {
        RepositoryServiceClient::new(self.channel.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn settings_client(&self) -> SettingsServiceClient<Channel> {
        SettingsServiceClient::new(self.channel.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn tag_client(&self) -> TagServiceClient<Channel> {
        TagServiceClient::new(self.channel.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
    }

    fn health_client(&self) -> HealthClient<Channel> {
        HealthClient::new(self.channel.clone())
            .max_encoding_message_size(MAX_REQUEST_MESSAGE_SIZE)
            .max_decoding_message_size(MAX_RESPONSE_MESSAGE_SIZE)
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::{error::Error, time::Duration};

    use gtl_local_transport::{LocalEndpoint, LocalListener};
    use gtl_wire::v1::{
        DiffTarget, Empty, PresentDiffRequest, PresentDiffResponse, PresentMergeDiffRequest,
        PresentMergeDiffResponse, PresentProjectRepositoryDiffsRequest,
        PresentProjectRepositoryDiffsResponse, PresentSubrepositoryDiffsRequest,
        PresentSubrepositoryDiffsResponse, RenderDiffRequest, RenderDiffResponse,
        RenderMergeDiffRequest, RenderMergeDiffResponse, RenderProjectRepositoryDiffsRequest,
        RenderProjectRepositoryDiffsResponse, RenderSubrepositoryDiffsRequest,
        RenderSubrepositoryDiffsResponse,
        diff_service_server::{DiffService, DiffServiceServer},
        diff_target, render_diff_response,
    };
    use tokio::sync::oneshot;
    use tonic::{Response, transport::Server};

    use super::*;

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    #[test]
    fn rpc_error_displays_the_public_message_and_retains_the_status_source() -> TestResult {
        let error = ClientError::from(Status::unavailable(
            "project catalogue is temporarily unavailable",
        ));

        assert_eq!(
            error.to_string(),
            "project catalogue is temporarily unavailable"
        );
        let status = error
            .source()
            .and_then(|source| source.downcast_ref::<Status>())
            .ok_or("RPC error omitted its Tonic status source")?;
        assert_eq!(status.code(), tonic::Code::Unavailable);
        Ok(())
    }

    #[test]
    fn viewer_server_info_rejects_invalid_wire_values() {
        let invalid_instance =
            ViewerServerInfo::try_from_response(&v1::GetViewerServerInfoResponse {
                server_instance_id: "not-a-uuid".to_owned(),
                protocol_version: 1,
                server_version: "0.1.0".into(),
            })
            .unwrap_err();
        assert!(matches!(
            invalid_instance,
            ViewerServerInfoError::InvalidInstanceId
        ));

        let invalid_protocol =
            ViewerServerInfo::try_from_response(&v1::GetViewerServerInfoResponse {
                server_instance_id: uuid::Uuid::new_v4().to_string(),
                protocol_version: 0,
                server_version: "0.1.0".into(),
            })
            .unwrap_err();
        assert!(matches!(
            invalid_protocol,
            ViewerServerInfoError::InvalidProtocolVersion
        ));
    }

    #[tokio::test]
    async fn connects_through_the_private_local_transport() -> TestResult {
        let harness = TestHarness::start().await?;

        let client = GtlClient::connect(&harness.endpoint).await?;

        client.check_health().await?;
        assert_eq!(client.endpoint(), &harness.endpoint);
        harness.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn sends_an_application_request_over_the_local_transport() -> TestResult {
        let harness = TestHarness::start().await?;
        let client = GtlClient::connect(&harness.endpoint).await?;

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
        async fn present_diff(
            &self,
            _request: Request<PresentDiffRequest>,
        ) -> Result<Response<PresentDiffResponse>, Status> {
            Ok(Response::new(PresentDiffResponse::default()))
        }

        async fn present_merge_diff(
            &self,
            _request: Request<PresentMergeDiffRequest>,
        ) -> Result<Response<PresentMergeDiffResponse>, Status> {
            Ok(Response::new(PresentMergeDiffResponse::default()))
        }

        async fn present_subrepository_diffs(
            &self,
            _request: Request<PresentSubrepositoryDiffsRequest>,
        ) -> Result<Response<PresentSubrepositoryDiffsResponse>, Status> {
            Ok(Response::new(PresentSubrepositoryDiffsResponse::default()))
        }

        async fn present_project_repository_diffs(
            &self,
            _request: Request<PresentProjectRepositoryDiffsRequest>,
        ) -> Result<Response<PresentProjectRepositoryDiffsResponse>, Status> {
            Ok(Response::new(
                PresentProjectRepositoryDiffsResponse::default(),
            ))
        }

        async fn render_diff(
            &self,
            _request: Request<RenderDiffRequest>,
        ) -> Result<Response<RenderDiffResponse>, Status> {
            Ok(Response::new(empty_diff_response()))
        }

        async fn render_merge_diff(
            &self,
            _request: Request<RenderMergeDiffRequest>,
        ) -> Result<Response<RenderMergeDiffResponse>, Status> {
            Ok(Response::new(RenderMergeDiffResponse::default()))
        }

        async fn render_subrepository_diffs(
            &self,
            _request: Request<RenderSubrepositoryDiffsRequest>,
        ) -> Result<Response<RenderSubrepositoryDiffsResponse>, Status> {
            Ok(Response::new(RenderSubrepositoryDiffsResponse::default()))
        }

        async fn render_project_repository_diffs(
            &self,
            _request: Request<RenderProjectRepositoryDiffsRequest>,
        ) -> Result<Response<RenderProjectRepositoryDiffsResponse>, Status> {
            Ok(Response::new(
                RenderProjectRepositoryDiffsResponse::default(),
            ))
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
        endpoint: LocalEndpoint,
        shutdown: oneshot::Sender<()>,
        task: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    }

    async fn run_test_server(
        listener: LocalListener,
        shutdown_receiver: oneshot::Receiver<()>,
        ready: oneshot::Sender<()>,
    ) -> Result<(), tonic::transport::Error> {
        let (incoming, _ownership) = listener.into_parts();
        let (health_reporter, health_service) = tonic_health::server::health_reporter();
        health_reporter
            .set_service_status("", tonic_health::ServingStatus::Serving)
            .await;
        let _ = ready.send(());
        Server::builder()
            .add_service(health_service)
            .add_service(DiffServiceServer::new(TestDiff))
            .serve_with_incoming_shutdown(incoming, async move {
                let _ = shutdown_receiver.await;
            })
            .await
    }

    impl TestHarness {
        async fn start() -> TestResult<Self> {
            let directory = tempfile::tempdir()?;
            let endpoint = LocalEndpoint::from_root(directory.path())?;
            let listener = LocalListener::bind(&endpoint, Duration::from_secs(1)).await?;
            let (shutdown, shutdown_receiver) = oneshot::channel();
            let (ready, ready_receiver) = oneshot::channel();
            let task = tokio::spawn(run_test_server(listener, shutdown_receiver, ready));
            ready_receiver
                .await
                .map_err(|_| "test server stopped before readiness")?;

            Ok(Self {
                _directory: directory,
                endpoint,
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
