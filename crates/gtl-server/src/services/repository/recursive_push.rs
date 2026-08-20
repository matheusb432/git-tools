use gtl_application::repositories::{
    apply_recursive_push::{self, ApplyRecursivePush},
    plan_recursive_push::{self, PlanRecursivePush},
};
use gtl_models::{
    git::{BranchName, RemoteName},
    paths::ProjectName,
    repository::recursive_push::{Dest, RepoOutcome, RepoTarget, SubreposPlan},
};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, repository_root, required, task_join, unexpected};
use crate::state::AppState;

const MAX_REPOSITORIES_PER_REQUEST: usize = 512;

pub(super) async fn plan(
    state: AppState,
    request: v1::PlanRecursivePushRequest,
) -> Result<Response<v1::PlanRecursivePushResponse>, Status> {
    let request = PlanRecursivePush {
        root: absolute_path(request.root, "root")?,
    };
    let result =
        tokio::task::spawn_blocking(move || plan_recursive_push::execute(request, &state.git))
            .await
            .map_err(|error| task_join(&error))?
            .map_err(|error| unexpected(error, "plan recursive repository push"))?;
    let outcome = match result {
        SubreposPlan::Ready(targets) => {
            v1::plan_recursive_push_response::Outcome::Ready(v1::RecursivePushPlan {
                targets: targets.into_iter().map(wire_target).collect(),
            })
        }
        SubreposPlan::Refused(detail) => {
            v1::plan_recursive_push_response::Outcome::Refused(v1::OperationDetail { detail })
        }
    };
    Ok(Response::new(v1::PlanRecursivePushResponse {
        outcome: Some(outcome),
    }))
}

pub(super) async fn execute(
    state: AppState,
    request: v1::ExecuteRecursivePushRequest,
) -> Result<Response<v1::ExecuteRecursivePushResponse>, Status> {
    if request.targets.len() > MAX_REPOSITORIES_PER_REQUEST {
        return Err(Status::resource_exhausted(format!(
            "targets cannot contain more than {MAX_REPOSITORIES_PER_REQUEST} entries"
        )));
    }
    let targets = request
        .targets
        .into_iter()
        .map(application_target)
        .collect::<Result<Vec<_>, _>>()?;
    let result = tokio::task::spawn_blocking(move || {
        apply_recursive_push::execute(ApplyRecursivePush { targets }, &state.git)
    })
    .await
    .map_err(|error| task_join(&error))?;
    let status = match result.status {
        gtl_models::repository::recursive_push::Status::Ok => v1::RecursivePushStatus::Ok,
        gtl_models::repository::recursive_push::Status::Partial => v1::RecursivePushStatus::Partial,
        gtl_models::repository::recursive_push::Status::Fail => v1::RecursivePushStatus::Failed,
    };
    let results = result
        .reports
        .into_iter()
        .map(|report| {
            let (status, detail) = match report.outcome {
                RepoOutcome::Pushed => (v1::RecursivePushRepositoryStatus::Pushed, None),
                RepoOutcome::UpToDate => (v1::RecursivePushRepositoryStatus::UpToDate, None),
                RepoOutcome::Skipped(detail) => {
                    (v1::RecursivePushRepositoryStatus::Skipped, Some(detail))
                }
                RepoOutcome::Failed(detail) => {
                    (v1::RecursivePushRepositoryStatus::Failed, Some(detail))
                }
            };
            v1::RecursivePushRepositoryResult {
                label: report.label.to_string(),
                status: status as i32,
                detail,
            }
        })
        .collect();
    Ok(Response::new(v1::ExecuteRecursivePushResponse {
        status: status as i32,
        results,
    }))
}

fn wire_target(target: RepoTarget) -> v1::RecursivePushTarget {
    let destination = match target.dest {
        Dest::Push { branch, remote } => {
            v1::recursive_push_target::Destination::Push(destination(&branch, &remote))
        }
        Dest::Synced { branch, remote } => {
            v1::recursive_push_target::Destination::Synced(destination(&branch, &remote))
        }
        Dest::Skip { reason } => {
            v1::recursive_push_target::Destination::Skip(v1::OperationDetail { detail: reason })
        }
    };
    v1::RecursivePushTarget {
        repository_root: target.path.to_string(),
        label: target.label.to_string(),
        destination: Some(destination),
    }
}

fn destination(branch: &BranchName, remote: &RemoteName) -> v1::PushDestination {
    v1::PushDestination {
        branch: branch.to_string(),
        remote: remote.to_string(),
    }
}

fn application_target(target: v1::RecursivePushTarget) -> Result<RepoTarget, Status> {
    let destination = required(target.destination, "targets.destination")?;
    let dest = match destination {
        v1::recursive_push_target::Destination::Push(destination) => {
            application_destination(destination, true)?
        }
        v1::recursive_push_target::Destination::Synced(destination) => {
            application_destination(destination, false)?
        }
        v1::recursive_push_target::Destination::Skip(reason) => Dest::Skip {
            reason: reason.detail,
        },
    };
    Ok(RepoTarget {
        path: repository_root(target.repository_root, "targets.repository_root")?,
        label: ProjectName::try_new(target.label)
            .map_err(|_| Status::invalid_argument("targets.label must not be empty"))?,
        dest,
    })
}

fn application_destination(destination: v1::PushDestination, push: bool) -> Result<Dest, Status> {
    let branch = BranchName::try_new(destination.branch)
        .map_err(|_| Status::invalid_argument("destination.branch must not be empty"))?;
    let remote = RemoteName::try_new(destination.remote)
        .map_err(|_| Status::invalid_argument("destination.remote must not be empty"))?;
    Ok(if push {
        Dest::Push { branch, remote }
    } else {
        Dest::Synced { branch, remote }
    })
}
