use gtl_application::repositories::{
    apply_commit::{self, ApplyCommit, CommitStatus},
    apply_push::{self, ApplyPush, ApplyPushOk, PushMode},
    plan_commit::{self, CommitTarget, PlanCommit, PlanCommitOk},
    plan_push::{self, PlanPush, PlanPushOk, PushTarget},
};
use gtl_models::{
    git::{BranchName, CommitCount, RemoteName, RemoteUrl},
    paths::ProjectName,
    repository::{PathCount, PendingChanges},
};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, repository_root, required, run_blocking, unexpected};
use crate::state::AppState;

pub(super) async fn plan_push(
    state: AppState,
    request: v1::PlanRepositoryPushRequest,
) -> Result<Response<v1::PlanRepositoryPushResponse>, Status> {
    let request = PlanPush {
        repo_path: absolute_path(request.repository_path, "repository_path")?,
    };
    let result = run_blocking(move || plan_push::execute(request, &state.git))
        .await?
        .map_err(|error| unexpected(error, "plan repository push"))?;
    let outcome = match result {
        PlanPushOk::Ready(target) => {
            v1::plan_repository_push_response::Outcome::Ready(push_target(target))
        }
        PlanPushOk::Refused(detail) => {
            v1::plan_repository_push_response::Outcome::Refused(v1::OperationDetail { detail })
        }
    };
    Ok(Response::new(v1::PlanRepositoryPushResponse {
        outcome: Some(outcome),
    }))
}

pub(super) async fn execute_push(
    state: AppState,
    request: v1::ExecuteRepositoryPushRequest,
) -> Result<Response<v1::ExecuteRepositoryPushResponse>, Status> {
    let target = application_push_target(required(request.target, "target")?)?;
    let mode = match v1::RepositoryPushMode::try_from(request.mode) {
        Ok(v1::RepositoryPushMode::ExistingCommits) => PushMode::ExistingOnly,
        Ok(v1::RepositoryPushMode::CommitChanges) => {
            let message = required(request.message, "message")?;
            if message.trim().is_empty() {
                return Err(Status::invalid_argument("message must not be empty"));
            }
            PushMode::CommitChanges { message }
        }
        Ok(v1::RepositoryPushMode::Unspecified) | Err(_) => {
            return Err(Status::invalid_argument("mode is invalid"));
        }
    };
    let result = run_blocking(move || apply_push::execute(ApplyPush { target, mode }, &state.git))
        .await?
        .map_err(|error| unexpected(error, "execute repository push"))?;
    let (status, detail) = match result {
        ApplyPushOk::Noop { detail } => (v1::RepositoryPushStatus::NoOp, detail),
        ApplyPushOk::Completed { detail, .. } => (v1::RepositoryPushStatus::Completed, detail),
        ApplyPushOk::Refused { detail } => (v1::RepositoryPushStatus::Refused, detail),
        ApplyPushOk::Failed { detail, .. } => (v1::RepositoryPushStatus::Failed, detail),
    };
    Ok(Response::new(v1::ExecuteRepositoryPushResponse {
        status: status as i32,
        detail,
    }))
}

pub(super) async fn plan_commit(
    state: AppState,
    request: v1::PlanRepositoryCommitRequest,
) -> Result<Response<v1::PlanRepositoryCommitResponse>, Status> {
    let request = PlanCommit {
        repo_path: absolute_path(request.repository_path, "repository_path")?,
    };
    let result = run_blocking(move || plan_commit::execute(request, &state.git))
        .await?
        .map_err(|error| unexpected(error, "plan repository commit"))?;
    let outcome = match result {
        PlanCommitOk::Ready(target) => {
            v1::plan_repository_commit_response::Outcome::Ready(commit_target(&target))
        }
        PlanCommitOk::Refused(detail) => {
            v1::plan_repository_commit_response::Outcome::Refused(v1::OperationDetail { detail })
        }
    };
    Ok(Response::new(v1::PlanRepositoryCommitResponse {
        outcome: Some(outcome),
    }))
}

pub(super) async fn execute_commit(
    state: AppState,
    request: v1::ExecuteRepositoryCommitRequest,
) -> Result<Response<v1::ExecuteRepositoryCommitResponse>, Status> {
    if request.message.trim().is_empty() {
        return Err(Status::invalid_argument("message must not be empty"));
    }
    let target = application_commit_target(required(request.target, "target")?)?;
    let result = run_blocking(move || {
        apply_commit::execute(
            ApplyCommit {
                target,
                message: request.message,
            },
            &state.git,
        )
    })
    .await?
    .map_err(|error| unexpected(error, "execute repository commit"))?;
    let status = match result.status {
        CommitStatus::Committed => v1::RepositoryCommitStatus::Committed,
        CommitStatus::Noop => v1::RepositoryCommitStatus::NoOp,
        CommitStatus::Failed => v1::RepositoryCommitStatus::Failed,
    };
    Ok(Response::new(v1::ExecuteRepositoryCommitResponse {
        status: status as i32,
        detail: result.detail,
    }))
}

fn push_target(target: PushTarget) -> v1::RepositoryPushTarget {
    v1::RepositoryPushTarget {
        project_name: target.name.to_string(),
        repository_root: target.top.to_string(),
        branch: target.branch.to_string(),
        remote: target.remote.to_string(),
        remote_url: target.remote_url.map(|url| url.to_string()),
        pending: Some(pending_changes(target.pending)),
    }
}

fn commit_target(target: &CommitTarget) -> v1::RepositoryCommitTarget {
    v1::RepositoryCommitTarget {
        project_name: target.name.to_string(),
        repository_root: target.top.to_string(),
        branch: target.branch.to_string(),
        pending: Some(pending_changes(target.pending)),
    }
}

fn pending_changes(pending: PendingChanges) -> v1::PendingChanges {
    v1::PendingChanges {
        changed_paths: pending.changed.value(),
        staged_paths: pending.staged.value(),
        unprepared_paths: pending.unprepared.value(),
        commits_ahead: pending.ahead.into_inner(),
    }
}

fn application_push_target(target: v1::RepositoryPushTarget) -> Result<PushTarget, Status> {
    Ok(PushTarget {
        name: project_name(target.project_name, "target.project_name")?,
        top: repository_root(target.repository_root, "target.repository_root")?,
        branch: branch_name(target.branch, "target.branch")?,
        remote: RemoteName::try_new(target.remote)
            .map_err(|_| Status::invalid_argument("target.remote must not be empty"))?,
        remote_url: target
            .remote_url
            .map(RemoteUrl::try_new)
            .transpose()
            .map_err(|_| Status::invalid_argument("target.remote_url must not be empty"))?,
        pending: application_pending(required(target.pending, "target.pending")?),
    })
}

fn application_commit_target(target: v1::RepositoryCommitTarget) -> Result<CommitTarget, Status> {
    Ok(CommitTarget {
        name: project_name(target.project_name, "target.project_name")?,
        top: repository_root(target.repository_root, "target.repository_root")?,
        branch: branch_name(target.branch, "target.branch")?,
        pending: application_pending(required(target.pending, "target.pending")?),
    })
}

fn application_pending(pending: v1::PendingChanges) -> PendingChanges {
    PendingChanges {
        changed: PathCount::new(pending.changed_paths),
        staged: PathCount::new(pending.staged_paths),
        unprepared: PathCount::new(pending.unprepared_paths),
        ahead: CommitCount::new(pending.commits_ahead),
    }
}

fn project_name(raw: String, field: &'static str) -> Result<ProjectName, Status> {
    ProjectName::try_new(raw)
        .map_err(|_| Status::invalid_argument(format!("{field} must not be empty")))
}

fn branch_name(raw: String, field: &'static str) -> Result<BranchName, Status> {
    BranchName::try_new(raw)
        .map_err(|_| Status::invalid_argument(format!("{field} must not be empty")))
}
