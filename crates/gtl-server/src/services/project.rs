mod catalogue;

use gtl_application::{
    ports::UserSettingsReader as _,
    projects::{
        RepoSyncResult, SyncExit, SyncStatus,
        catalogue::{
            create_project, get_project, list_active_projects,
            set_project_membership::{self, ProjectMembership, SetProjectMembership},
            set_project_status::{self, SetProjectStatus},
        },
        commit_repositories::{
            self, CommitAction, CommitExit, CommitRepositories, CommitRepositoriesError,
            CommitRepositoriesMode, CommitRepositoriesOk, CommitRepositoriesScope, CommitResult,
        },
        pull_repositories::{self, PullRepositoriesError},
        push_repositories::{self, PushRepositoriesError},
    },
    repositories::get_repository_statuses,
};
use gtl_models::{
    git::GitEffectMode,
    paths::ProjectName,
    projects::catalogue::{ProjectIds, ProjectStatus},
    repository::traversal::RepositoryTarget,
};
use gtl_wire::v1::{self, project_service_server::ProjectService};
use tonic::{Request, Response, Status};

use super::{
    project_client_error, repository::status_results, run_blocking, unexpected,
    user_settings_load_error,
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
            create_project::execute(&request, &mut connection).map_err(catalogue::error)
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
            .map_err(catalogue::invalid)?;
        let state = self.state.clone();
        let project = run_blocking(move || {
            let connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            get_project::execute(&id, &connection).map_err(catalogue::error)
        })
        .await??;
        Ok(Response::new(catalogue::get_response(project)))
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
            list_active_projects::execute((), &connection).map_err(catalogue::error)
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
            id: request.project_id.try_into().map_err(catalogue::invalid)?,
            status: ProjectStatus::Paused,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let outcome = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_status::execute(&request, &mut connection).map_err(catalogue::error)
        })
        .await??;
        Ok(Response::new(v1::PauseProjectResponse {
            outcome: catalogue::outcome(outcome),
        }))
    }

    async fn resume_project(
        &self,
        request: Request<v1::ResumeProjectRequest>,
    ) -> Result<Response<v1::ResumeProjectResponse>, Status> {
        let request = request.into_inner();
        let request = SetProjectStatus {
            id: request.project_id.try_into().map_err(catalogue::invalid)?,
            status: ProjectStatus::Active,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let outcome = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_status::execute(&request, &mut connection).map_err(catalogue::error)
        })
        .await??;
        Ok(Response::new(v1::ResumeProjectResponse {
            outcome: catalogue::outcome(outcome),
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
            .map_err(catalogue::invalid)?;
        let request = SetProjectMembership {
            ids: ProjectIds::try_new(ids).map_err(catalogue::invalid)?,
            membership: ProjectMembership::Managed,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let mutations = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_membership::execute(&request, &mut connection).map_err(catalogue::error)
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
            .map_err(catalogue::invalid)?;
        let request = SetProjectMembership {
            ids: ProjectIds::try_new(ids).map_err(catalogue::invalid)?,
            membership: ProjectMembership::Unmanaged,
            mode: catalogue::mode(request.mode)?,
        };
        let state = self.state.clone();
        let mutations = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(|error| catalogue::lock_error(&error))?;
            set_project_membership::execute(&request, &mut connection).map_err(catalogue::error)
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
        .map_err(push_error)?;

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
        .map_err(pull_error)?;

        Ok(Response::new(
            sync_response(result.results, result.exit).into(),
        ))
    }

    async fn commit_project_repositories(
        &self,
        request: Request<v1::CommitProjectRepositoriesRequest>,
    ) -> Result<Response<v1::CommitProjectRepositoriesResponse>, Status> {
        let request = request.into_inner();
        let state = self.state.clone();
        let scope = if request.use_push_all_exclusions {
            CommitRepositoriesScope::PushAll {
                exclusions: state
                    .user_settings
                    .load()
                    .map_err(user_settings_load_error)?
                    .push_all_exclusions()
                    .clone(),
            }
        } else {
            CommitRepositoriesScope::All
        };
        let repos = state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_client_error(&error))?;
        let mode = if request.dry_run {
            CommitRepositoriesMode::DryRun
        } else {
            CommitRepositoriesMode::Apply {
                message: request.message,
            }
        };
        let result = run_blocking(move || {
            commit_repositories::execute(CommitRepositories { repos, mode, scope }, &state.git)
        })
        .await?;

        Ok(Response::new(commit_response(result)))
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
            .map_err(|error| project_client_error(&error))?
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

fn push_error(error: PushRepositoriesError) -> Status {
    match error {
        PushRepositoriesError::ProjectClient(error) => project_client_error(&error),
        PushRepositoriesError::Settings(error) => user_settings_load_error(error),
        PushRepositoriesError::Unexpected(error) => unexpected(error, "push project repositories"),
    }
}

fn pull_error(error: PullRepositoriesError) -> Status {
    match error {
        PullRepositoriesError::ProjectClient(error) => project_client_error(&error),
        PullRepositoriesError::Unexpected(error) => unexpected(error, "pull project repositories"),
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

fn sync_result(result: RepoSyncResult) -> v1::RepositorySyncResult {
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

fn commit_response(
    result: Result<CommitRepositoriesOk, CommitRepositoriesError>,
) -> v1::CommitProjectRepositoriesResponse {
    match result {
        Ok(result) => v1::CommitProjectRepositoriesResponse {
            results: result.results.iter().map(commit_result).collect(),
            exit: match result.exit {
                CommitExit::Clean => v1::ProjectCommitExit::Clean,
                CommitExit::Warn => v1::ProjectCommitExit::Warning,
                CommitExit::Fail => v1::ProjectCommitExit::Failed,
            } as i32,
            failure_detail: None,
        },
        Err(error) => {
            let failure_detail = error.to_string();
            tracing::error!(error = ?error, "project commit stopped before completion");
            let completed_results = match error {
                CommitRepositoriesError::Transport {
                    mut completed_results,
                    failed_result,
                    ..
                } => {
                    completed_results.extend(failed_result.map(|result| *result));
                    completed_results
                }
                _ => Vec::new(),
            };
            v1::CommitProjectRepositoriesResponse {
                results: completed_results.iter().map(commit_result).collect(),
                exit: v1::ProjectCommitExit::Failed as i32,
                failure_detail: Some(failure_detail),
            }
        }
    }
}

fn commit_result(result: &CommitResult) -> v1::ProjectCommitResult {
    v1::ProjectCommitResult {
        project_name: result.name().to_string(),
        present: result.is_present(),
        dirty: result.is_dirty(),
        files: result
            .files()
            .iter()
            .map(|file| v1::CommitFile {
                status: file.status.clone(),
                path: file.path.as_ref().to_string_lossy().into_owned(),
            })
            .collect(),
        action: match result.action() {
            CommitAction::Absent => v1::ProjectCommitAction::Absent,
            CommitAction::Clean => v1::ProjectCommitAction::Clean,
            CommitAction::WouldCommit => v1::ProjectCommitAction::WouldCommit,
            CommitAction::Skipped => v1::ProjectCommitAction::Skipped,
            CommitAction::Committed => v1::ProjectCommitAction::Committed,
            CommitAction::Fail => v1::ProjectCommitAction::Failed,
        } as i32,
        detail: result.detail().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::projects::commit_repositories::CommitOutcome;
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
            vec![ProjectName::try_new("sample_project").unwrap()],
            SyncExit::Warn,
        );

        assert_eq!(response.exit(), v1::ProjectSyncExit::Warning);
        assert_eq!(
            response.selected[0].status(),
            v1::RepositorySyncStatus::WouldPush
        );
        assert_eq!(response.excluded_project_names, ["sample_project"]);
    }

    #[test]
    fn invalid_push_settings_map_to_failed_precondition() {
        let status = push_error(PushRepositoriesError::Settings(
            gtl_application::ports::UserSettingsConfigurationError::new(
                "/tmp/config.toml".into(),
                anyhow::anyhow!("bad project settings"),
            )
            .into(),
        ));

        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            status.message(),
            "user settings at /tmp/config.toml are invalid: bad project settings"
        );
    }

    #[test]
    fn commit_projection_preserves_closed_result_facts() {
        let result = CommitResult::new(ProjectName::try_new("api").unwrap(), CommitOutcome::Clean);
        let projected = commit_result(&result);

        assert_eq!(projected.project_name, "api");
        assert!(projected.present);
        assert!(!projected.dirty);
        assert_eq!(projected.action(), v1::ProjectCommitAction::Clean);
        assert_eq!(projected.detail, "nothing to commit");
    }
}
