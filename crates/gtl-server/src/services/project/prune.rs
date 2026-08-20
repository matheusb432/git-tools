use gtl_application::{
    projects::prune_branches::{
        self, PruneAction, PruneBranches, PruneBranchesError, PruneBranchesOk, PruneExit,
        PruneRepoResult,
    },
    repositories::{apply_prune::PruneFailure, plan_prune::PruneBranch},
};
use gtl_models::git::{BranchName, GitEffectMode};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{project_client_error, task_join};
use crate::state::AppState;

pub(super) async fn execute(
    state: AppState,
    request: v1::PruneProjectBranchesRequest,
) -> Result<Response<v1::PruneProjectBranchesResponse>, Status> {
    let onto = BranchName::try_new(request.onto_branch)
        .map_err(|_| Status::invalid_argument("onto_branch must not be empty"))?;
    let repos = state
        .projects
        .list_projects()
        .await
        .map_err(|error| project_client_error(&error))?;
    let mode = if request.dry_run {
        GitEffectMode::DryRun
    } else {
        GitEffectMode::Apply
    };
    let result = tokio::task::spawn_blocking(move || {
        prune_branches::execute(PruneBranches { repos, onto, mode }, &state.git)
    })
    .await
    .map_err(|error| task_join(&error))?;

    Ok(Response::new(prune_response(result)))
}

fn prune_response(
    result: Result<PruneBranchesOk, PruneBranchesError>,
) -> v1::PruneProjectBranchesResponse {
    match result {
        Ok(result) => v1::PruneProjectBranchesResponse {
            results: result.results.into_iter().map(prune_result).collect(),
            exit: match result.exit {
                PruneExit::Clean => v1::ProjectPruneExit::Clean,
                PruneExit::Warn => v1::ProjectPruneExit::Warning,
            } as i32,
            failure_detail: None,
        },
        Err(error) => {
            let failure_detail = error.to_string();
            tracing::error!(error = ?error, "project prune stopped before completion");
            let completed_results = match error {
                PruneBranchesError::Transport {
                    mut completed_results,
                    failed_result,
                    ..
                } => {
                    if let Some(failed_result) = failed_result {
                        completed_results.push(*failed_result);
                    }
                    completed_results
                }
                _ => Vec::new(),
            };
            v1::PruneProjectBranchesResponse {
                results: completed_results.into_iter().map(prune_result).collect(),
                exit: v1::ProjectPruneExit::Warning as i32,
                failure_detail: Some(failure_detail),
            }
        }
    }
}

fn prune_result(result: PruneRepoResult) -> v1::ProjectPruneResult {
    let (action, deleted, failures, detail) = match result.action {
        PruneAction::Absent => (
            v1::ProjectPruneAction::Absent,
            Vec::new(),
            Vec::new(),
            String::new(),
        ),
        PruneAction::Refused(detail) => (
            v1::ProjectPruneAction::Refused,
            Vec::new(),
            Vec::new(),
            detail,
        ),
        PruneAction::Nothing(detail) => (
            v1::ProjectPruneAction::Nothing,
            Vec::new(),
            Vec::new(),
            detail,
        ),
        PruneAction::WouldDelete(branches) => (
            v1::ProjectPruneAction::WouldDelete,
            branches.iter().map(wire_branch).collect(),
            Vec::new(),
            String::new(),
        ),
        PruneAction::Applied(applied) => (
            v1::ProjectPruneAction::Applied,
            applied.deleted.iter().map(wire_branch).collect(),
            applied.failed.into_iter().map(wire_failure).collect(),
            String::new(),
        ),
    };
    v1::ProjectPruneResult {
        project_name: result.name.to_string(),
        action: action as i32,
        deleted,
        failures,
        detail,
    }
}

fn wire_branch(branch: &PruneBranch) -> v1::PruneBranch {
    v1::PruneBranch {
        name: branch.name.to_string(),
        commit_id: branch.id.to_string(),
    }
}

fn wire_failure(failure: PruneFailure) -> v1::PruneFailure {
    v1::PruneFailure {
        name: failure.name.to_string(),
        reason: failure.reason,
    }
}
