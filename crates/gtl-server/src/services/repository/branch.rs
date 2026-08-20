use std::path::PathBuf;

use gtl_application::repositories::{
    apply_rebase::{self, ApplyRebase, ApplyRebaseOk},
    apply_revert::{self, ApplyRevert, ApplyRevertOk},
    apply_switch::{self, ApplySwitch, SwitchStatus},
    plan_rebase::{self, PlanRebase, PlanRebaseOk},
    plan_revert::{self, PlanRevert, PlanRevertOk},
    plan_switch::{self, PlanSwitch, PlanSwitchOk},
};
use gtl_models::git::BranchName;
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, task_join, unexpected};
use crate::state::AppState;

pub(super) async fn change(
    state: AppState,
    request: v1::ChangeRepositoryBranchRequest,
) -> Result<Response<v1::ChangeRepositoryBranchResponse>, Status> {
    let repository_path = absolute_path(request.repository_path, "repository_path")?;
    let onto = BranchName::try_new(request.onto_branch)
        .map_err(|_| Status::invalid_argument("onto_branch must not be empty"))?;
    let action = v1::RepositoryBranchAction::try_from(request.action)
        .map_err(|_| Status::invalid_argument("action is invalid"))?;
    if action == v1::RepositoryBranchAction::Unspecified {
        return Err(Status::invalid_argument("action is invalid"));
    }
    let response =
        tokio::task::spawn_blocking(move || change_blocking(&state, repository_path, onto, action))
            .await
            .map_err(|error| task_join(&error))??;
    Ok(Response::new(response))
}

fn change_blocking(
    state: &AppState,
    repository_path: PathBuf,
    onto: BranchName,
    action: v1::RepositoryBranchAction,
) -> Result<v1::ChangeRepositoryBranchResponse, Status> {
    match action {
        v1::RepositoryBranchAction::Switch => switch(state, repository_path, onto),
        v1::RepositoryBranchAction::Rebase => rebase(state, repository_path, onto),
        v1::RepositoryBranchAction::Revert => revert(state, repository_path, onto),
        v1::RepositoryBranchAction::Unspecified => {
            Err(Status::invalid_argument("action is invalid"))
        }
    }
}

fn switch(
    state: &AppState,
    repository_path: PathBuf,
    onto: BranchName,
) -> Result<v1::ChangeRepositoryBranchResponse, Status> {
    match plan_switch::execute(
        PlanSwitch {
            repo_path: repository_path,
            onto,
        },
        &state.git,
    )
    .map_err(|error| unexpected(error, "plan repository branch switch"))?
    {
        PlanSwitchOk::Refused(detail) => Ok(response(v1::RepositoryBranchStatus::Refused, detail)),
        PlanSwitchOk::AlreadyThere(onto) => Ok(response(
            v1::RepositoryBranchStatus::AlreadyThere,
            format!("already on '{onto}'"),
        )),
        PlanSwitchOk::Ready(target) => {
            let result = apply_switch::execute(ApplySwitch { target }, &state.git)
                .map_err(|error| unexpected(error, "execute repository branch switch"))?;
            Ok(response(
                match result.status {
                    SwitchStatus::Switched => v1::RepositoryBranchStatus::Switched,
                    SwitchStatus::Failed => v1::RepositoryBranchStatus::Failed,
                },
                result.detail,
            ))
        }
    }
}

fn rebase(
    state: &AppState,
    repository_path: PathBuf,
    onto: BranchName,
) -> Result<v1::ChangeRepositoryBranchResponse, Status> {
    match plan_rebase::execute(
        PlanRebase {
            repo_path: repository_path,
            onto,
        },
        &state.git,
    )
    .map_err(|error| unexpected(error, "plan repository fast-forward"))?
    {
        PlanRebaseOk::Refused(detail) => Ok(response(v1::RepositoryBranchStatus::Refused, detail)),
        PlanRebaseOk::Noop(detail) => Ok(response(v1::RepositoryBranchStatus::NoOp, detail)),
        PlanRebaseOk::Ready(target) => {
            let result = apply_rebase::execute(ApplyRebase { target }, &state.git)
                .map_err(|error| unexpected(error, "execute repository fast-forward"))?;
            Ok(match result {
                ApplyRebaseOk::FastForwarded { detail, .. } => {
                    response(v1::RepositoryBranchStatus::FastForwarded, detail)
                }
                ApplyRebaseOk::Failed { detail, .. } => {
                    response(v1::RepositoryBranchStatus::Failed, detail)
                }
            })
        }
    }
}

fn revert(
    state: &AppState,
    repository_path: PathBuf,
    onto: BranchName,
) -> Result<v1::ChangeRepositoryBranchResponse, Status> {
    match plan_revert::execute(
        PlanRevert {
            repo_path: repository_path,
            onto,
        },
        &state.git,
    )
    .map_err(|error| unexpected(error, "plan repository branch recovery"))?
    {
        PlanRevertOk::Refused(detail) => Ok(response(v1::RepositoryBranchStatus::Refused, detail)),
        PlanRevertOk::Ready(target) => {
            let result = apply_revert::execute(ApplyRevert { target }, &state.git)
                .map_err(|error| unexpected(error, "execute repository branch recovery"))?;
            Ok(match result {
                ApplyRevertOk::Reverted { detail } => {
                    response(v1::RepositoryBranchStatus::Reverted, detail)
                }
                ApplyRevertOk::Failed { detail, .. } => {
                    response(v1::RepositoryBranchStatus::Failed, detail)
                }
            })
        }
    }
}

fn response(
    status: v1::RepositoryBranchStatus,
    detail: String,
) -> v1::ChangeRepositoryBranchResponse {
    v1::ChangeRepositoryBranchResponse {
        status: status as i32,
        detail,
    }
}
