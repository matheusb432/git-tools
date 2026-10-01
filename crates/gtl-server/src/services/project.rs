mod catalogue;

use gtl_application::{
    projects::{
        RepoSyncResult, SyncExit, SyncStatus,
        catalogue::{
            create_project, get_project, list_projects,
            set_project_membership::{self, ProjectMembership, SetProjectMembership},
        },
        get_project_repository::{self, GetProjectRepository},
        pull_repositories, push_repositories,
        set_project_status::{self, SetProjectStatus},
    },
    repositories::get_repository_statuses,
};
use gtl_models::{
    git::GitEffectMode,
    paths::ProjectName,
    projects::catalogue::{ProjectIds, ProjectStatus, ProjectStatusFilter},
    repository::traversal::RepositoryTarget,
};
use gtl_wire::v1::{self, project_service_server::ProjectService};
use tonic::{Request, Response, Status};

use super::{
    repository::status_results,
    run_blocking,
    status::{GrpcResultExt as _, invalid_request},
};
use crate::state::AppState;

pub(crate) struct ProjectGrpcService {
    state: AppState,
}

impl ProjectGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl ProjectService for ProjectGrpcService {
    async fn create_project(
        &self,
        request: Request<v1::CreateProjectRequest>,
    ) -> Result<Response<v1::CreateProjectResponse>, Status> {
        let request = catalogue::create_request(request.into_inner())?;
        let state = self.state.clone();
        let id = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            create_project::execute(&request, &mut connection).into_grpc()
        })
        .await??;
        Ok(Response::new(v1::CreateProjectResponse {
            project_id: id.to_string(),
        }))
    }

    async fn get_project(
        &self,
        request: Request<v1::GetProjectRequest>,
    ) -> Result<Response<v1::GetProjectResponse>, Status> {
        let id = request
            .into_inner()
            .project_id
            .try_into()
            .map_err(|_| invalid_request("project_id"))?;
        let state = self.state.clone();
        let project = run_blocking(move || {
            let connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            get_project::execute(&id, &connection).into_grpc()
        })
        .await??;
        Ok(Response::new(catalogue::get_response(project)))
    }

    async fn get_project_repository(
        &self,
        request: Request<v1::GetProjectRepositoryRequest>,
    ) -> Result<Response<v1::GetProjectRepositoryResponse>, Status> {
        let request = GetProjectRepository {
            id: request
                .into_inner()
                .project_id
                .try_into()
                .map_err(|_| invalid_request("project_id"))?,
        };
        let state = self.state.clone();
        let repository = run_blocking(move || {
            let connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            get_project_repository::execute(&request, &connection).into_grpc()
        })
        .await??;
        Ok(Response::new(v1::GetProjectRepositoryResponse {
            repository_root: repository.to_string(),
        }))
    }

    async fn list_active_projects(
        &self,
        _: Request<v1::ListActiveProjectsRequest>,
    ) -> Result<Response<v1::ListActiveProjectsResponse>, Status> {
        let state = self.state.clone();
        let projects = run_blocking(move || {
            let connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            list_projects::execute(ProjectStatusFilter::Active, &connection).into_grpc()
        })
        .await??;
        Ok(Response::new(v1::ListActiveProjectsResponse {
            projects: projects.into_iter().map(catalogue::project).collect(),
        }))
    }

    async fn pause_project(
        &self,
        request: Request<v1::PauseProjectRequest>,
    ) -> Result<Response<v1::PauseProjectResponse>, Status> {
        let request = request.into_inner();
        let request = SetProjectStatus {
            id: request
                .project_id
                .try_into()
                .map_err(|_| invalid_request("project_id"))?,
            status: ProjectStatus::Paused,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let result = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_status::execute(&request, &mut connection).into_grpc()
        })
        .await??;
        Ok(Response::new(v1::PauseProjectResponse {
            outcome: catalogue::outcome(result.outcome),
            target_status: catalogue::status(result.target_status),
        }))
    }

    async fn resume_project(
        &self,
        request: Request<v1::ResumeProjectRequest>,
    ) -> Result<Response<v1::ResumeProjectResponse>, Status> {
        let request = request.into_inner();
        let request = SetProjectStatus {
            id: request
                .project_id
                .try_into()
                .map_err(|_| invalid_request("project_id"))?,
            status: ProjectStatus::Active,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let result = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_status::execute(&request, &mut connection).into_grpc()
        })
        .await??;
        Ok(Response::new(v1::ResumeProjectResponse {
            outcome: catalogue::outcome(result.outcome),
            target_status: catalogue::status(result.target_status),
        }))
    }

    async fn manage_projects(
        &self,
        request: Request<v1::ManageProjectsRequest>,
    ) -> Result<Response<v1::ManageProjectsResponse>, Status> {
        let request = request.into_inner();
        let ids = request
            .project_ids
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid_request("project_ids"))?;
        let request = SetProjectMembership {
            ids: ProjectIds::try_new(ids).map_err(|_| invalid_request("project_ids"))?,
            membership: ProjectMembership::Managed,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let mutations = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_membership::execute(&request, &mut connection).into_grpc()
        })
        .await??;
        Ok(Response::new(v1::ManageProjectsResponse {
            mutations: catalogue::mutations(mutations),
        }))
    }

    async fn unmanage_projects(
        &self,
        request: Request<v1::UnmanageProjectsRequest>,
    ) -> Result<Response<v1::UnmanageProjectsResponse>, Status> {
        let request = request.into_inner();
        let ids = request
            .project_ids
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid_request("project_ids"))?;
        let request = SetProjectMembership {
            ids: ProjectIds::try_new(ids).map_err(|_| invalid_request("project_ids"))?,
            membership: ProjectMembership::Unmanaged,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let mutations = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_membership::execute(&request, &mut connection).into_grpc()
        })
        .await??;
        Ok(Response::new(v1::UnmanageProjectsResponse {
            mutations: catalogue::mutations(mutations),
        }))
    }

    async fn push_project_repositories(
        &self,
        request: Request<v1::PushProjectRepositoriesRequest>,
    ) -> Result<Response<v1::PushProjectRepositoriesResponse>, Status> {
        let state = self.state.clone();
        let result = push_repositories::execute(
            effect_mode(request.into_inner().dry_run),
            &state.git,
            &state.projects,
            &state.clock,
            &state.user_settings,
        )
        .await
        .into_grpc()?;

        Ok(Response::new(push_response(
            result.selected,
            result.excluded,
            result.exit,
        )))
    }

    async fn pull_project_repositories(
        &self,
        request: Request<v1::PullProjectRepositoriesRequest>,
    ) -> Result<Response<v1::PullProjectRepositoriesResponse>, Status> {
        let state = self.state.clone();
        let result = pull_repositories::execute(
            effect_mode(request.into_inner().dry_run),
            &state.git,
            &state.projects,
        )
        .await
        .into_grpc()?;

        Ok(Response::new(
            sync_response(result.results, result.exit).into(),
        ))
    }

    async fn get_project_repository_statuses(
        &self,
        _request: Request<v1::GetProjectRepositoryStatusesRequest>,
    ) -> Result<Response<v1::GetProjectRepositoryStatusesResponse>, Status> {
        let state = self.state.clone();
        let repos = state
            .projects
            .list_projects()
            .await
            .into_grpc()?
            .into_iter()
            .map(|repo| RepositoryTarget {
                label: repo.name,
                path: repo.path,
            })
            .collect();
        let results =
            run_blocking(move || get_repository_statuses::execute(repos, &state.git)).await?;

        Ok(Response::new(v1::GetProjectRepositoryStatusesResponse {
            results: status_results(&results),
        }))
    }
}

const fn effect_mode(dry_run: bool) -> GitEffectMode {
    if dry_run {
        GitEffectMode::DryRun
    } else {
        GitEffectMode::Apply
    }
}

struct ProjectRepositorySyncSummary {
    results: Vec<v1::RepositorySyncResult>,
    exit: i32,
}

fn sync_response(results: Vec<RepoSyncResult>, exit: SyncExit) -> ProjectRepositorySyncSummary {
    ProjectRepositorySyncSummary {
        results: results.into_iter().map(sync_result).collect(),
        exit: sync_exit(exit),
    }
}

fn push_response(
    selected: Vec<RepoSyncResult>,
    excluded: Vec<ProjectName>,
    exit: SyncExit,
) -> v1::PushProjectRepositoriesResponse {
    v1::PushProjectRepositoriesResponse {
        selected: selected.into_iter().map(sync_result).collect(),
        exit: sync_exit(exit),
        excluded_project_names: excluded.into_iter().map(|name| name.to_string()).collect(),
    }
}

fn sync_exit(exit: SyncExit) -> i32 {
    (match exit {
        SyncExit::Clean => v1::ProjectSyncExit::Clean,
        SyncExit::Warn => v1::ProjectSyncExit::Warning,
        SyncExit::Fail => v1::ProjectSyncExit::Failed,
    }) as i32
}

impl From<ProjectRepositorySyncSummary> for v1::PullProjectRepositoriesResponse {
    fn from(response: ProjectRepositorySyncSummary) -> Self {
        Self {
            results: response.results,
            exit: response.exit,
        }
    }
}

pub(super) fn sync_result(result: RepoSyncResult) -> v1::RepositorySyncResult {
    v1::RepositorySyncResult {
        project_name: result.name.to_string(),
        branch: result.branch.map(|branch| branch.to_string()),
        status: match result.status {
            SyncStatus::Skip => v1::RepositorySyncStatus::Skip,
            SyncStatus::UpToDate => v1::RepositorySyncStatus::UpToDate,
            SyncStatus::Pushed => v1::RepositorySyncStatus::Pushed,
            SyncStatus::WouldPush => v1::RepositorySyncStatus::WouldPush,
            SyncStatus::Pulled => v1::RepositorySyncStatus::Pulled,
            SyncStatus::WouldPull => v1::RepositorySyncStatus::WouldPull,
            SyncStatus::Warn => v1::RepositorySyncStatus::Warning,
            SyncStatus::Fail => v1::RepositorySyncStatus::Failed,
        } as i32,
        detail: result.detail,
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::projects::push_repositories::PushRepositoriesError;
    use gtl_models::git::BranchName;

    use super::*;

    #[test]
    fn sync_projection_preserves_the_closed_status_and_exit() {
        let response = push_response(
            vec![RepoSyncResult {
                name: ProjectName::try_new("git-tools").unwrap(),
                branch: Some(BranchName::try_new("main").unwrap()),
                status: SyncStatus::WouldPush,
                detail: "ahead by 2".into(),
            }],
            vec![ProjectName::try_new("example-project").unwrap()],
            SyncExit::Warn,
        );

        assert_eq!(response.exit(), v1::ProjectSyncExit::Warning);
        assert_eq!(
            response.selected[0].status(),
            v1::RepositorySyncStatus::WouldPush
        );
        assert_eq!(response.excluded_project_names, ["example-project"]);
    }

    #[test]
    fn invalid_push_settings_map_to_failed_precondition() {
        let status = super::super::status::status(&PushRepositoriesError::Settings(
            gtl_application::ports::UserSettingsConfigurationError::new(
                "//fixture.invalid/repositories/tmp/config.toml".into(),
                anyhow::anyhow!("bad project settings"),
            )
            .into(),
        ));

        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            super::super::status::decoded_failure(&status),
            Some(gtl_models::failure::Failure::Settings(
                gtl_models::failure::SettingsFailure::Invalid {
                    path: "//fixture.invalid/repositories/tmp/config.toml".into(),
                    diagnostic: gtl_models::failure::ExternalDiagnostic::new(
                        "bad project settings"
                    ),
                }
            ))
        );
    }
}
