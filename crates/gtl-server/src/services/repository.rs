mod branch;
mod prune;
mod push;
mod recursive_push;
pub(crate) mod status;

use gtl_wire::v1::{self, repository_service_server::RepositoryService};
use tonic::{Request, Response, Status};

use crate::state::AppState;

#[derive(Clone)]
pub(crate) struct RepositoryApi {
    state: AppState,
}

impl RepositoryApi {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl RepositoryService for RepositoryApi {
    async fn plan_repository_push(
        &self,
        request: Request<v1::PlanRepositoryPushRequest>,
    ) -> Result<Response<v1::PlanRepositoryPushResponse>, Status> {
        push::plan_push(self.state.clone(), request.into_inner()).await
    }

    async fn execute_repository_push(
        &self,
        request: Request<v1::ExecuteRepositoryPushRequest>,
    ) -> Result<Response<v1::ExecuteRepositoryPushResponse>, Status> {
        push::execute_push(self.state.clone(), request.into_inner()).await
    }

    async fn plan_repository_commit(
        &self,
        request: Request<v1::PlanRepositoryCommitRequest>,
    ) -> Result<Response<v1::PlanRepositoryCommitResponse>, Status> {
        push::plan_commit(self.state.clone(), request.into_inner()).await
    }

    async fn execute_repository_commit(
        &self,
        request: Request<v1::ExecuteRepositoryCommitRequest>,
    ) -> Result<Response<v1::ExecuteRepositoryCommitResponse>, Status> {
        push::execute_commit(self.state.clone(), request.into_inner()).await
    }

    async fn plan_recursive_repository_push(
        &self,
        request: Request<v1::PlanRecursiveRepositoryPushRequest>,
    ) -> Result<Response<v1::PlanRecursiveRepositoryPushResponse>, Status> {
        recursive_push::plan(self.state.clone(), request.into_inner()).await
    }

    async fn execute_recursive_repository_push(
        &self,
        request: Request<v1::ExecuteRecursiveRepositoryPushRequest>,
    ) -> Result<Response<v1::ExecuteRecursiveRepositoryPushResponse>, Status> {
        recursive_push::execute(self.state.clone(), request.into_inner()).await
    }

    async fn change_repository_branch(
        &self,
        request: Request<v1::ChangeRepositoryBranchRequest>,
    ) -> Result<Response<v1::ChangeRepositoryBranchResponse>, Status> {
        branch::change(self.state.clone(), request.into_inner()).await
    }

    async fn plan_repository_prune(
        &self,
        request: Request<v1::PlanRepositoryPruneRequest>,
    ) -> Result<Response<v1::PlanRepositoryPruneResponse>, Status> {
        prune::plan(self.state.clone(), request.into_inner()).await
    }

    async fn execute_repository_prune(
        &self,
        request: Request<v1::ExecuteRepositoryPruneRequest>,
    ) -> Result<Response<v1::ExecuteRepositoryPruneResponse>, Status> {
        prune::execute(self.state.clone(), request.into_inner()).await
    }

    async fn get_repository_status(
        &self,
        request: Request<v1::GetRepositoryStatusRequest>,
    ) -> Result<Response<v1::GetRepositoryStatusResponse>, Status> {
        status::get(self.state.clone(), request.into_inner()).await
    }

    async fn get_recursive_repository_statuses(
        &self,
        request: Request<v1::GetRecursiveRepositoryStatusesRequest>,
    ) -> Result<Response<v1::GetRecursiveRepositoryStatusesResponse>, Status> {
        status::get_recursive(self.state.clone(), request.into_inner()).await
    }
}
