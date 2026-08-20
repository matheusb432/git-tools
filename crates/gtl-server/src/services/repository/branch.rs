use gtl_application::repositories::change_branch::{
    self, ChangeBranch, ChangeBranchAction, ChangeBranchOk,
};
use gtl_models::git::BranchName;
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, run_blocking, unexpected};
use crate::state::AppState;

pub(super) async fn change(
    state: AppState,
    request: v1::ChangeRepositoryBranchRequest,
) -> Result<Response<v1::ChangeRepositoryBranchResponse>, Status> {
    let repo_path = absolute_path(request.repository_path, "repository_path")?;
    let onto = BranchName::try_new(request.onto_branch)
        .map_err(|_| Status::invalid_argument("onto_branch must not be empty"))?;
    let action = action(request.action)?;
    let result = run_blocking(move || {
        change_branch::execute(
            ChangeBranch {
                repo_path,
                onto,
                action,
            },
            &state.git,
        )
    })
    .await?
    .map_err(|error| unexpected(error, "change repository branch"))?;

    Ok(Response::new(response(result)))
}

fn action(raw: i32) -> Result<ChangeBranchAction, Status> {
    match v1::RepositoryBranchAction::try_from(raw)
        .map_err(|_| Status::invalid_argument("action is invalid"))?
    {
        v1::RepositoryBranchAction::Switch => Ok(ChangeBranchAction::Switch),
        v1::RepositoryBranchAction::Rebase => Ok(ChangeBranchAction::Rebase),
        v1::RepositoryBranchAction::Revert => Ok(ChangeBranchAction::Revert),
        v1::RepositoryBranchAction::Unspecified => {
            Err(Status::invalid_argument("action is required"))
        }
    }
}

fn response(result: ChangeBranchOk) -> v1::ChangeRepositoryBranchResponse {
    let (status, detail) = match result {
        ChangeBranchOk::Switched { detail } => (v1::RepositoryBranchStatus::Switched, detail),
        ChangeBranchOk::AlreadyThere { detail } => {
            (v1::RepositoryBranchStatus::AlreadyThere, detail)
        }
        ChangeBranchOk::FastForwarded { detail } => {
            (v1::RepositoryBranchStatus::FastForwarded, detail)
        }
        ChangeBranchOk::Reverted { detail } => (v1::RepositoryBranchStatus::Reverted, detail),
        ChangeBranchOk::NoOp { detail } => (v1::RepositoryBranchStatus::NoOp, detail),
        ChangeBranchOk::Refused { detail } => (v1::RepositoryBranchStatus::Refused, detail),
        ChangeBranchOk::Failed { detail } => (v1::RepositoryBranchStatus::Failed, detail),
    };
    v1::ChangeRepositoryBranchResponse {
        status: status as i32,
        detail,
    }
}
