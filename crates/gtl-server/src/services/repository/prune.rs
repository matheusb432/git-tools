use gtl_application::repositories::{
    apply_prune::{self, ApplyPrune, ApplyPruneError, ApplyPruneOk, PruneStatus},
    plan_prune::{self, PlanPrune, PlanPruneOk, PruneBranch},
};
use gtl_models::{diffs::CommitId, git::BranchName};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, repository_root, required, run_blocking, unexpected};
use crate::state::AppState;

const MAX_BRANCHES_PER_REQUEST: usize = 512;

pub(super) async fn plan(
    state: AppState,
    request: v1::PlanRepositoryPruneRequest,
) -> Result<Response<v1::PlanRepositoryPruneResponse>, Status> {
    let request = PlanPrune {
        repo_path: absolute_path(request.repository_path, "repository_path")?,
        onto: BranchName::try_new(request.onto_branch)
            .map_err(|_| Status::invalid_argument("onto_branch must not be empty"))?,
    };
    let result = run_blocking(move || plan_prune::execute(request, &state.git))
        .await?
        .map_err(|error| unexpected(error, "plan repository branch prune"))?;
    let outcome = match result {
        PlanPruneOk::Ready { top, branches, .. } => {
            v1::plan_repository_prune_response::Outcome::Ready(v1::RepositoryPrunePlan {
                repository_root: top.to_string(),
                branches: branches.iter().map(wire_branch).collect(),
            })
        }
        PlanPruneOk::Nothing(detail) => {
            v1::plan_repository_prune_response::Outcome::Nothing(v1::OperationDetail { detail })
        }
        PlanPruneOk::Refused(detail) => {
            v1::plan_repository_prune_response::Outcome::Refused(v1::OperationDetail { detail })
        }
    };
    Ok(Response::new(v1::PlanRepositoryPruneResponse {
        outcome: Some(outcome),
    }))
}

pub(super) async fn execute(
    state: AppState,
    request: v1::ExecuteRepositoryPruneRequest,
) -> Result<Response<v1::ExecuteRepositoryPruneResponse>, Status> {
    let plan = required(request.plan, "plan")?;
    if plan.branches.len() > MAX_BRANCHES_PER_REQUEST {
        return Err(Status::resource_exhausted(format!(
            "plan.branches cannot contain more than {MAX_BRANCHES_PER_REQUEST} entries"
        )));
    }
    let command = ApplyPrune {
        top: repository_root(plan.repository_root, "plan.repository_root")?,
        branches: plan
            .branches
            .into_iter()
            .map(application_branch)
            .collect::<Result<Vec<_>, _>>()?,
    };
    let result = run_blocking(move || apply_prune::execute(command, &state.git)).await?;
    Ok(Response::new(match result {
        Ok(result) => applied_response(result, None),
        Err(error) => aborted_response(error)?,
    }))
}

fn aborted_response(error: ApplyPruneError) -> Result<v1::ExecuteRepositoryPruneResponse, Status> {
    let detail = error.to_string();
    tracing::error!(error = ?error, "repository branch prune stopped before completion");
    match error {
        ApplyPruneError::Transport {
            completed_result, ..
        } => Ok(match completed_result {
            None => v1::ExecuteRepositoryPruneResponse {
                status: v1::RepositoryPruneStatus::Aborted as i32,
                deleted: Vec::new(),
                failures: Vec::new(),
                failure_detail: Some(detail),
            },
            Some(result) => applied_response(*result, Some(detail)),
        }),
        _ => Err(Status::internal("execute repository branch prune failed")),
    }
}

fn applied_response(
    result: ApplyPruneOk,
    failure_detail: Option<String>,
) -> v1::ExecuteRepositoryPruneResponse {
    let status = failure_detail.as_ref().map_or_else(
        || match result.status {
            PruneStatus::Ok => v1::RepositoryPruneStatus::Ok,
            PruneStatus::Partial => v1::RepositoryPruneStatus::Partial,
            PruneStatus::Fail => v1::RepositoryPruneStatus::Failed,
        },
        |_| v1::RepositoryPruneStatus::Aborted,
    );
    v1::ExecuteRepositoryPruneResponse {
        status: status as i32,
        deleted: result.deleted.iter().map(wire_branch).collect(),
        failures: result
            .failed
            .into_iter()
            .map(|failure| v1::PruneFailure {
                name: failure.name.to_string(),
                reason: failure.reason,
            })
            .collect(),
        failure_detail,
    }
}

fn wire_branch(branch: &PruneBranch) -> v1::PruneBranch {
    v1::PruneBranch {
        name: branch.name.to_string(),
        commit_id: branch.id.to_string(),
    }
}

fn application_branch(branch: v1::PruneBranch) -> Result<PruneBranch, Status> {
    Ok(PruneBranch {
        name: BranchName::try_new(branch.name)
            .map_err(|_| Status::invalid_argument("plan.branches.name must not be empty"))?,
        id: CommitId::try_from(branch.commit_id)
            .map_err(|_| Status::invalid_argument("plan.branches.commit_id is invalid"))?,
    })
}
