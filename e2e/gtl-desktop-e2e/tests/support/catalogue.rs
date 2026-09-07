use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result};
use sample_project_local_auth::{LocalAuth, ServerEndpoint, ServerInstanceId};
use sample_project_wire::v1::{self, project_service_server::ProjectServiceServer};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, transport::Server};

#[derive(Clone, Default)]
pub struct CatalogueState {
    projects: Arc<Mutex<Vec<v1::Project>>>,
    requests: Arc<AtomicU64>,
    response_delay_ms: Arc<AtomicU64>,
}

pub struct ProjectCatalogue {
    pub state: CatalogueState,
    task: JoinHandle<Result<()>>,
}

impl ProjectCatalogue {
    pub async fn start(data_root: &Path, state: CatalogueState) -> Result<Self> {
        let auth = LocalAuth::from_data_root(data_root)?;
        let _capabilities = auth.load_or_create_server_capabilities()?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint =
            ServerEndpoint::try_new(listener.local_addr()?, ServerInstanceId::generate())?;
        let published = auth.publish_endpoint(endpoint)?;
        let service = ProjectServiceServer::new(ProjectCatalogueService(state.clone()));
        let (health, health_service) = tonic_health::server::health_reporter();
        health
            .set_serving::<ProjectServiceServer<ProjectCatalogueService>>()
            .await;
        let task = tokio::spawn(async move {
            let _published = published;
            Server::builder()
                .add_service(health_service)
                .add_service(service)
                .serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener))
                .await
                .context("serve isolated sample_project catalogue")
        });
        Ok(Self { state, task })
    }

    pub fn requests(&self) -> u64 {
        self.state.requests.load(Ordering::SeqCst)
    }

    pub fn delay_responses(&self, milliseconds: u64) {
        self.state
            .response_delay_ms
            .store(milliseconds, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.task.abort();
    }

    pub fn set_projects(&self, projects: &[(&str, &str, &Path)]) -> Result<()> {
        let mut current = self
            .state
            .projects
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture catalogue lock failed"))?;
        let fixture_home = std::env::var_os("HOME").context("isolated fixture home")?;
        *current = projects
            .iter()
            .map(|(id, title, path)| -> Result<v1::Project> {
                let relative = path
                    .strip_prefix(&fixture_home)
                    .context("fixture projects must be inside the isolated home")?;
                Ok(v1::Project {
                    id: (*id).to_owned(),
                    title: (*title).to_owned(),
                    source: Some(v1::ProjectSource {
                        source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                            path: format!("~/{}", relative.display()),
                        })),
                    }),
                    git_remote: None,
                    mux_session_name: id.to_ascii_lowercase(),
                    status: v1::ProjectStatus::Active.into(),
                    affiliation: v1::ProjectAffiliation::Personal.into(),
                    color: None,
                    groups: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(())
    }
}

impl Drop for ProjectCatalogue {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct ProjectCatalogueService(CatalogueState);

#[tonic::async_trait]
impl v1::project_service_server::ProjectService for ProjectCatalogueService {
    async fn get_project(
        &self,
        _request: Request<v1::GetProjectRequest>,
    ) -> Result<Response<v1::GetProjectResponse>, Status> {
        Err(Status::not_found("unknown fixture project"))
    }

    async fn list_active_projects(
        &self,
        _request: Request<v1::ListActiveProjectsRequest>,
    ) -> Result<Response<v1::ListActiveProjectsResponse>, Status> {
        self.0.requests.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(
            self.0.response_delay_ms.load(Ordering::SeqCst),
        ))
        .await;
        Ok(Response::new(v1::ListActiveProjectsResponse {
            projects: self
                .0
                .projects
                .lock()
                .map_err(|_| Status::internal("fixture catalogue lock failed"))?
                .clone(),
        }))
    }

    async fn pause_project(
        &self,
        _request: Request<v1::PauseProjectRequest>,
    ) -> Result<Response<v1::PauseProjectResponse>, Status> {
        Err(Status::unimplemented("pause_project"))
    }

    async fn resume_project(
        &self,
        _request: Request<v1::ResumeProjectRequest>,
    ) -> Result<Response<v1::ResumeProjectResponse>, Status> {
        Err(Status::unimplemented("resume_project"))
    }

    async fn manage_projects(
        &self,
        _request: Request<v1::ManageProjectsRequest>,
    ) -> Result<Response<v1::ManageProjectsResponse>, Status> {
        Err(Status::unimplemented("manage_projects"))
    }

    async fn unmanage_projects(
        &self,
        _request: Request<v1::UnmanageProjectsRequest>,
    ) -> Result<Response<v1::UnmanageProjectsResponse>, Status> {
        Err(Status::unimplemented("unmanage_projects"))
    }

    async fn prepare_project_clones(
        &self,
        _request: Request<v1::PrepareProjectClonesRequest>,
    ) -> Result<Response<v1::PrepareProjectClonesResponse>, Status> {
        Err(Status::unimplemented("prepare_project_clones"))
    }

    type ExportProjectStream = ReceiverStream<Result<v1::ExportProjectResponse, Status>>;

    async fn export_project(
        &self,
        _request: Request<v1::ExportProjectRequest>,
    ) -> Result<Response<Self::ExportProjectStream>, Status> {
        Err(Status::unimplemented("export_project"))
    }

    type ExportAllProjectsStream = ReceiverStream<Result<v1::ExportAllProjectsResponse, Status>>;

    async fn export_all_projects(
        &self,
        _request: Request<v1::ExportAllProjectsRequest>,
    ) -> Result<Response<Self::ExportAllProjectsStream>, Status> {
        Err(Status::unimplemented("export_all_projects"))
    }

    async fn add_project_session(
        &self,
        _request: Request<v1::AddProjectSessionRequest>,
    ) -> Result<Response<v1::AddProjectSessionResponse>, Status> {
        Err(Status::unimplemented("add_project_session"))
    }

    async fn prepare_project_session_entry(
        &self,
        _request: Request<v1::PrepareProjectSessionEntryRequest>,
    ) -> Result<Response<v1::PrepareProjectSessionEntryResponse>, Status> {
        Err(Status::unimplemented("prepare_project_session_entry"))
    }

    async fn list_project_sessions(
        &self,
        _request: Request<v1::ListProjectSessionsRequest>,
    ) -> Result<Response<v1::ListProjectSessionsResponse>, Status> {
        Err(Status::unimplemented("list_project_sessions"))
    }

    type KillProjectSessionStream = ReceiverStream<Result<v1::KillProjectSessionResponse, Status>>;

    async fn kill_project_session(
        &self,
        _request: Request<tonic::Streaming<v1::KillProjectSessionRequest>>,
    ) -> Result<Response<Self::KillProjectSessionStream>, Status> {
        Err(Status::unimplemented("kill_project_session"))
    }

    async fn style_project_sessions(
        &self,
        _request: Request<v1::StyleProjectSessionsRequest>,
    ) -> Result<Response<v1::StyleProjectSessionsResponse>, Status> {
        Err(Status::unimplemented("style_project_sessions"))
    }

    async fn create_project(
        &self,
        _request: Request<v1::CreateProjectRequest>,
    ) -> Result<Response<v1::CreateProjectResponse>, Status> {
        Err(Status::unimplemented("create_project"))
    }
}
