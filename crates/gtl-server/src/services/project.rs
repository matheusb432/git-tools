mod commit;
mod prune;
mod status;

use gtl_application::projects::{
    RepoSyncResult, SyncExit, SyncStatus,
    pull_repositories::{self, PullRepositories, PullRepositoriesError},
    push_repositories::{self, PushRepositories, PushRepositoriesError},
};
use gtl_models::git::GitEffectMode;
use gtl_wire::v1::{self, project_service_server::ProjectService};
use tonic::{Request, Response, Status};

use super::{project_client_error, unexpected};
use crate::state::AppState;

#[derive(Clone)]
pub(crate) struct ProjectApi {
    state: AppState,
}

impl ProjectApi {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl ProjectService for ProjectApi {
    async fn push_repositories(
        &self,
        request: Request<v1::SyncProjectsRequest>,
    ) -> Result<Response<v1::SyncProjectsResponse>, Status> {
        let state = self.state.clone();
        let result = push_repositories::execute(
            PushRepositories {
                mode: effect_mode(request.into_inner().dry_run),
            },
            &state.git,
            &state.projects,
            &state.clock,
        )
        .await
        .map_err(push_error)?;

        Ok(Response::new(sync_response(result.results, result.exit)))
    }

    async fn pull_repositories(
        &self,
        request: Request<v1::SyncProjectsRequest>,
    ) -> Result<Response<v1::SyncProjectsResponse>, Status> {
        let state = self.state.clone();
        let result = pull_repositories::execute(
            PullRepositories {
                mode: effect_mode(request.into_inner().dry_run),
            },
            &state.git,
            &state.projects,
        )
        .await
        .map_err(pull_error)?;

        Ok(Response::new(sync_response(result.results, result.exit)))
    }

    async fn commit_repositories(
        &self,
        request: Request<v1::CommitProjectRepositoriesRequest>,
    ) -> Result<Response<v1::CommitProjectRepositoriesResponse>, Status> {
        commit::execute(self.state.clone(), request.into_inner()).await
    }

    async fn prune_branches(
        &self,
        request: Request<v1::PruneProjectBranchesRequest>,
    ) -> Result<Response<v1::PruneProjectBranchesResponse>, Status> {
        prune::execute(self.state.clone(), request.into_inner()).await
    }

    async fn get_statuses(
        &self,
        _request: Request<v1::Empty>,
    ) -> Result<Response<v1::RepositoryStatusesResponse>, Status> {
        status::get(self.state.clone()).await
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
        PushRepositoriesError::Unexpected(error) => unexpected(error, "push project repositories"),
    }
}

fn pull_error(error: PullRepositoriesError) -> Status {
    match error {
        PullRepositoriesError::ProjectClient(error) => project_client_error(&error),
        PullRepositoriesError::Unexpected(error) => unexpected(error, "pull project repositories"),
    }
}

fn sync_response(results: Vec<RepoSyncResult>, exit: SyncExit) -> v1::SyncProjectsResponse {
    v1::SyncProjectsResponse {
        results: results.into_iter().map(sync_result).collect(),
        exit: match exit {
            SyncExit::Clean => v1::ProjectSyncExit::Clean,
            SyncExit::Warn => v1::ProjectSyncExit::Warning,
            SyncExit::Fail => v1::ProjectSyncExit::Failed,
        } as i32,
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

#[cfg(test)]
mod tests {
    use gtl_models::{git::BranchName, paths::ProjectName};

    use super::*;

    #[test]
    fn sync_projection_preserves_the_closed_status_and_exit() {
        let response = sync_response(
            vec![RepoSyncResult {
                name: ProjectName::try_new("git-tools").unwrap(),
                branch: Some(BranchName::try_new("main").unwrap()),
                status: SyncStatus::WouldPush,
                detail: "ahead by 2".into(),
            }],
            SyncExit::Warn,
        );

        assert_eq!(response.exit(), v1::ProjectSyncExit::Warning);
        assert_eq!(
            response.results[0].status(),
            v1::RepositorySyncStatus::WouldPush
        );
    }
}
