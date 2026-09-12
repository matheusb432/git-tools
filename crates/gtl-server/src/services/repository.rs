use gtl_application::repositories::{
    apply_push::{self, ApplyPush, ApplyPushError, ApplyPushOk, PushBasis, PushMode, PushProgress},
    apply_recursive_push,
    get_recursive_repository_statuses::{
        self, GetRecursiveRepositoryStatuses, GetRecursiveRepositoryStatusesError,
    },
    get_repository_status,
    plan_push::{self, PlanPushOk, PushTarget},
    plan_recursive_push,
    pull_repository::{self, PullRepository},
};
use gtl_models::{
    git::{BranchName, CommitCount, RemoteName, RemoteUrl},
    paths::ProjectName,
    repository::{
        PathCount, PendingChanges,
        recursive_push::{Dest, RepoOutcome, RepoTarget, SubreposPlan},
        status::StatusResult,
        traversal::RepositoryTraversalScope,
    },
};
use gtl_wire::v1::{self, repository_service_server::RepositoryService};
use tonic::{Request, Response, Status};

use super::{
    absolute_path, repository_resolution_error, repository_root, required, run_blocking, unexpected,
};
use crate::state::AppState;

const MAX_REPOSITORIES_PER_REQUEST: usize = 512;

pub(crate) struct RepositoryGrpcService {
    state: AppState,
}

impl RepositoryGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl RepositoryService for RepositoryGrpcService {
    async fn plan_repository_push(
        &self,
        request: Request<v1::PlanRepositoryPushRequest>,
    ) -> Result<Response<v1::PlanRepositoryPushResponse>, Status> {
        let request = absolute_path(request.into_inner().repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = run_blocking(move || plan_push::execute(&request, &state.git))
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

    async fn execute_repository_push(
        &self,
        request: Request<v1::ExecuteRepositoryPushRequest>,
    ) -> Result<Response<v1::ExecuteRepositoryPushResponse>, Status> {
        let request = request.into_inner();
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
        let state = self.state.clone();
        let result =
            run_blocking(move || apply_push::execute(ApplyPush { target, mode }, &state.git))
                .await?;
        Ok(Response::new(push_response(result)?))
    }

    async fn plan_recursive_repository_push(
        &self,
        request: Request<v1::PlanRecursiveRepositoryPushRequest>,
    ) -> Result<Response<v1::PlanRecursiveRepositoryPushResponse>, Status> {
        let request = absolute_path(request.into_inner().root, "root")?;
        let state = self.state.clone();
        let result = run_blocking(move || plan_recursive_push::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "plan recursive repository push"))?;
        let outcome = match result {
            SubreposPlan::Ready(targets) => {
                v1::plan_recursive_repository_push_response::Outcome::Ready(v1::RecursivePushPlan {
                    targets: targets.into_iter().map(recursive_push_target).collect(),
                })
            }
            SubreposPlan::Refused(detail) => {
                v1::plan_recursive_repository_push_response::Outcome::Refused(v1::OperationDetail {
                    detail,
                })
            }
        };

        Ok(Response::new(v1::PlanRecursiveRepositoryPushResponse {
            outcome: Some(outcome),
        }))
    }

    async fn execute_recursive_repository_push(
        &self,
        request: Request<v1::ExecuteRecursiveRepositoryPushRequest>,
    ) -> Result<Response<v1::ExecuteRecursiveRepositoryPushResponse>, Status> {
        let request = request.into_inner();
        if request.targets.len() > MAX_REPOSITORIES_PER_REQUEST {
            return Err(Status::resource_exhausted(format!(
                "targets cannot contain more than {MAX_REPOSITORIES_PER_REQUEST} entries"
            )));
        }
        let targets = request
            .targets
            .into_iter()
            .map(application_recursive_push_target)
            .collect::<Result<Vec<_>, _>>()?;
        let state = self.state.clone();
        let result =
            run_blocking(move || apply_recursive_push::execute(targets, &state.git)).await?;
        let status = match result.status {
            gtl_models::repository::recursive_push::Status::Ok => v1::RecursivePushStatus::Ok,
            gtl_models::repository::recursive_push::Status::Partial => {
                v1::RecursivePushStatus::Partial
            }
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

        Ok(Response::new(v1::ExecuteRecursiveRepositoryPushResponse {
            status: status as i32,
            results,
        }))
    }

    async fn pull_repository(
        &self,
        request: Request<v1::PullRepositoryRequest>,
    ) -> Result<Response<v1::PullRepositoryResponse>, Status> {
        let request = request.into_inner();
        let request = PullRepository {
            path: absolute_path(request.repository_path, "repository_path")?,
            mode: if request.dry_run {
                gtl_models::git::GitEffectMode::DryRun
            } else {
                gtl_models::git::GitEffectMode::Apply
            },
        };
        let state = self.state.clone();
        let result = run_blocking(move || pull_repository::execute(request, &state.git))
            .await?
            .map_err(repository_resolution_error)?;
        Ok(Response::new(v1::PullRepositoryResponse {
            result: Some(super::project::sync_result(result)),
        }))
    }

    async fn get_repository_status(
        &self,
        request: Request<v1::GetRepositoryStatusRequest>,
    ) -> Result<Response<v1::GetRepositoryStatusResponse>, Status> {
        let request = absolute_path(request.into_inner().repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = run_blocking(move || get_repository_status::execute(request, &state.git))
            .await?
            .map_err(repository_resolution_error)?;

        Ok(Response::new(v1::GetRepositoryStatusResponse {
            results: status_results(std::slice::from_ref(&result)),
        }))
    }

    async fn get_recursive_repository_statuses(
        &self,
        request: Request<v1::GetRecursiveRepositoryStatusesRequest>,
    ) -> Result<Response<v1::GetRecursiveRepositoryStatusesResponse>, Status> {
        let request = GetRecursiveRepositoryStatuses {
            root: absolute_path(request.into_inner().root, "root")?,
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        };
        let state = self.state.clone();
        let results =
            run_blocking(move || get_recursive_repository_statuses::execute(request, &state.git))
                .await?
                .map_err(recursive_repository_statuses_error)?;

        Ok(Response::new(v1::GetRecursiveRepositoryStatusesResponse {
            results: status_results(&results),
        }))
    }
}

fn push_target(target: PushTarget) -> v1::RepositoryPushTarget {
    v1::RepositoryPushTarget {
        project_name: target.name.to_string(),
        repository_root: target.top.to_string(),
        branch: target.branch.to_string(),
        remote: target.remote.to_string(),
        remote_urls: target
            .remote_urls
            .into_iter()
            .map(|url| url.to_string())
            .collect(),
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
        remote_urls: target
            .remote_urls
            .into_iter()
            .map(RemoteUrl::try_new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Status::invalid_argument("target.remote_urls must not be empty"))?,
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

fn recursive_push_target(target: RepoTarget) -> v1::RecursivePushTarget {
    let destination = match target.dest {
        Dest::Push { branch, remote } => {
            v1::recursive_push_target::Destination::Push(push_destination(&branch, &remote))
        }
        Dest::Synced { branch, remote } => {
            v1::recursive_push_target::Destination::Synced(push_destination(&branch, &remote))
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

fn push_destination(branch: &BranchName, remote: &RemoteName) -> v1::PushDestination {
    v1::PushDestination {
        branch: branch.to_string(),
        remote: remote.to_string(),
    }
}

fn application_recursive_push_target(
    target: v1::RecursivePushTarget,
) -> Result<RepoTarget, Status> {
    let destination = required(target.destination, "targets.destination")?;
    let dest = match destination {
        v1::recursive_push_target::Destination::Push(destination) => {
            application_push_destination(destination, true)?
        }
        v1::recursive_push_target::Destination::Synced(destination) => {
            application_push_destination(destination, false)?
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

fn application_push_destination(
    destination: v1::PushDestination,
    push: bool,
) -> Result<Dest, Status> {
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

fn recursive_repository_statuses_error(error: GetRecursiveRepositoryStatusesError) -> Status {
    match error {
        GetRecursiveRepositoryStatusesError::NoRepositories { root } => Status::not_found(format!(
            "no git repositories found under {}",
            root.display()
        )),
        error => unexpected(error, "get recursive repository statuses"),
    }
}

pub(super) fn status_results(results: &[StatusResult]) -> Vec<v1::RepositoryStatusResult> {
    results
        .iter()
        .map(gtl_wire::proto::viewer::projects::encode_status_result)
        .collect()
}

fn push_response(
    result: Result<ApplyPushOk, ApplyPushError>,
) -> Result<v1::ExecuteRepositoryPushResponse, Status> {
    let (status, detail, progress) = match result {
        Ok(ApplyPushOk::Noop { detail }) => (v1::RepositoryPushStatus::NoOp, detail, None),
        Ok(ApplyPushOk::Completed { detail, .. }) => {
            (v1::RepositoryPushStatus::Completed, detail, None)
        }
        Ok(ApplyPushOk::Refused { detail }) => (v1::RepositoryPushStatus::Refused, detail, None),
        Ok(ApplyPushOk::Failed { detail, progress }) => (
            v1::RepositoryPushStatus::Failed,
            detail,
            Some(push_progress(&progress)),
        ),
        Err(error) => {
            let detail = error.to_string();
            match error {
                ApplyPushError::Transport { progress, .. } => (
                    v1::RepositoryPushStatus::Failed,
                    detail,
                    Some(push_progress(&progress)),
                ),
                error => return Err(unexpected(error, "execute repository push")),
            }
        }
    };
    Ok(v1::ExecuteRepositoryPushResponse {
        status: status as i32,
        detail,
        progress,
    })
}

fn push_progress(progress: &PushProgress) -> v1::RepositoryMutationProgress {
    use v1::repository_mutation_progress::State;
    let state = match progress {
        PushProgress::NotStarted => State::Unchanged(v1::Empty {}),
        PushProgress::Staged => State::Staged(v1::Empty {}),
        PushProgress::CommitCreated { id } => State::CreatedCommitId(id.to_string()),
        PushProgress::PushAttempted { basis } => State::PushAttempted(v1::RepositoryPushAttempt {
            created_commit_id: match basis {
                PushBasis::ExistingCommits => None,
                PushBasis::CreatedCommit { id } => Some(id.to_string()),
            },
        }),
    };
    v1::RepositoryMutationProgress { state: Some(state) }
}
