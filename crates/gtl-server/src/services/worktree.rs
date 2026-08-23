use gtl_application::worktrees::{
    get_worktree_base::{self, GetWorktreeBase, GetWorktreeBaseError, GetWorktreeBaseOk},
    list_worktrees::{self, ListWorktrees, ListWorktreesError, ListWorktreesOk},
};
use gtl_models::worktrees::{Worktree, WorktreeCheckout, WorktreeKind};
use gtl_wire::v1::{self, worktree_service_server::WorktreeService};
use tonic::{Request, Response, Status};

use super::{absolute_path, repository_resolution_error, run_blocking, unexpected};
use crate::state::AppState;

#[derive(Clone)]
pub(crate) struct WorktreeApi {
    state: AppState,
}

impl WorktreeApi {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl WorktreeService for WorktreeApi {
    async fn get_worktree_base(
        &self,
        request: Request<v1::GetWorktreeBaseRequest>,
    ) -> Result<Response<v1::GetWorktreeBaseResponse>, Status> {
        let repo_path = absolute_path(request.into_inner().repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            get_worktree_base::execute(GetWorktreeBase { repo_path }, &state.git)
        })
        .await?
        .map_err(get_base_error)?;
        let outcome = match result {
            GetWorktreeBaseOk::Found { path } => {
                v1::get_worktree_base_response::Outcome::Found(v1::WorktreePath {
                    path: path.to_string(),
                })
            }
            GetWorktreeBaseOk::Failed { detail } => {
                v1::get_worktree_base_response::Outcome::Failed(v1::WorktreeFailure { detail })
            }
        };
        Ok(Response::new(v1::GetWorktreeBaseResponse {
            outcome: Some(outcome),
        }))
    }

    async fn list_worktrees(
        &self,
        request: Request<v1::ListWorktreesRequest>,
    ) -> Result<Response<v1::ListWorktreesResponse>, Status> {
        let repo_path = absolute_path(request.into_inner().repository_path, "repository_path")?;
        let state = self.state.clone();
        let result =
            run_blocking(move || list_worktrees::execute(ListWorktrees { repo_path }, &state.git))
                .await?
                .map_err(list_error)?;
        let outcome = match result {
            ListWorktreesOk::Listed { worktrees } => {
                v1::list_worktrees_response::Outcome::Listed(v1::WorktreeList {
                    worktrees: worktrees.iter().map(wire_worktree).collect(),
                })
            }
            ListWorktreesOk::Failed { detail } => {
                v1::list_worktrees_response::Outcome::Failed(v1::WorktreeFailure { detail })
            }
        };
        Ok(Response::new(v1::ListWorktreesResponse {
            outcome: Some(outcome),
        }))
    }
}

fn get_base_error(error: GetWorktreeBaseError) -> Status {
    match error {
        GetWorktreeBaseError::Resolve(error) => repository_resolution_error(error),
        error => unexpected(error, "get primary worktree"),
    }
}

fn list_error(error: ListWorktreesError) -> Status {
    match error {
        ListWorktreesError::Resolve(error) => repository_resolution_error(error),
        error => unexpected(error, "list worktrees"),
    }
}

fn wire_worktree(worktree: &Worktree) -> v1::Worktree {
    let kind = Some(match worktree.kind() {
        WorktreeKind::Checkout(WorktreeCheckout::Branch(branch)) => {
            v1::worktree::Kind::Branch(branch.to_string())
        }
        WorktreeKind::Checkout(WorktreeCheckout::Detached) => {
            v1::worktree::Kind::Detached(v1::Empty {})
        }
        WorktreeKind::Bare => v1::worktree::Kind::Bare(v1::Empty {}),
    });
    v1::Worktree {
        path: worktree.path().to_string(),
        commit_id: worktree.id().to_string(),
        kind,
        locked: worktree.locked().map(str::to_owned),
        prunable: worktree.prunable().map(str::to_owned),
    }
}
