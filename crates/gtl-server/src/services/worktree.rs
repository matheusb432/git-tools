use gtl_application::{
    repositories::resolve_repository_root,
    worktrees::{
        get_worktree_base::{self, GetWorktreeBase, GetWorktreeBaseOk},
        list_worktrees::{self, ListWorktrees, ListWorktreesOk},
    },
};
use gtl_models::worktrees::{Worktree, WorktreeCheckout, WorktreeKind};
use gtl_wire::v1::{self, worktree_service_server::WorktreeService};
use tonic::{Request, Response, Status};

use super::{absolute_path, task_join, unexpected};
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
    async fn get_base(
        &self,
        request: Request<v1::GetWorktreeBaseRequest>,
    ) -> Result<Response<v1::GetWorktreeBaseResponse>, Status> {
        let repo_path = absolute_path(request.into_inner().repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = tokio::task::spawn_blocking(move || {
            let repo_path = resolve_repository_root::execute(
                resolve_repository_root::ResolveRepositoryRoot { repo_path },
                &state.git,
            )?;
            get_worktree_base::execute(GetWorktreeBase { repo_path }, &state.git)
                .map_err(anyhow::Error::from)
        })
        .await
        .map_err(|error| task_join(&error))?
        .map_err(|error| unexpected(error, "get primary worktree"))?;
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

    async fn list(
        &self,
        request: Request<v1::ListWorktreesRequest>,
    ) -> Result<Response<v1::ListWorktreesResponse>, Status> {
        let repo_path = absolute_path(request.into_inner().repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = tokio::task::spawn_blocking(move || {
            let repo_path = resolve_repository_root::execute(
                resolve_repository_root::ResolveRepositoryRoot { repo_path },
                &state.git,
            )?;
            list_worktrees::execute(ListWorktrees { repo_path }, &state.git)
                .map_err(anyhow::Error::from)
        })
        .await
        .map_err(|error| task_join(&error))?
        .map_err(|error| unexpected(error, "list worktrees"))?;
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
