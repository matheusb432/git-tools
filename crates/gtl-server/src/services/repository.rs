use gtl_application::repositories::{
    apply_commit::{self, ApplyCommit, CommitStatus},
    apply_prune::{self, ApplyPrune, ApplyPruneError, ApplyPruneOk, PruneStatus},
    apply_push::{self, ApplyPush, ApplyPushOk, PushMode},
    apply_recursive_push,
    change_branch::{self, ChangeBranch, ChangeBranchAction, ChangeBranchOk},
    get_recursive_repository_statuses::{
        self, GetRecursiveRepositoryStatuses, GetRecursiveRepositoryStatusesError,
    },
    get_repository_status,
    plan_commit::{self, CommitTarget, PlanCommitOk},
    plan_prune::{self, PlanPrune, PlanPruneOk, PruneBranch},
    plan_push::{self, PlanPushOk, PushTarget},
    plan_recursive_push,
};
use gtl_models::{
    diffs::CommitId,
    git::{BranchName, CommitCount, RemoteName, RemoteUrl},
    paths::ProjectName,
    repository::{
        PathCount, PendingChanges,
        recursive_push::{Dest, RepoOutcome, RepoTarget, SubreposPlan},
        status::{RepositoryStatus, StatusChanges, StatusHead, StatusResult, StatusUpstream},
        traversal::RepositoryTraversalScope,
    },
};
use gtl_wire::v1::{self, repository_service_server::RepositoryService};
use tonic::{Request, Response, Status};

use super::{
    absolute_path, repository_resolution_error, repository_root, required, run_blocking, unexpected,
};
use crate::state::AppState;

const MAX_BRANCHES_PER_REQUEST: usize = 512;
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

    async fn plan_repository_commit(
        &self,
        request: Request<v1::PlanRepositoryCommitRequest>,
    ) -> Result<Response<v1::PlanRepositoryCommitResponse>, Status> {
        let request = absolute_path(request.into_inner().repository_path, "repository_path")?;
        let state = self.state.clone();
        let result = run_blocking(move || plan_commit::execute(&request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "plan repository commit"))?;
        let outcome = match result {
            PlanCommitOk::Ready(target) => {
                v1::plan_repository_commit_response::Outcome::Ready(commit_target(&target))
            }
            PlanCommitOk::Refused(detail) => {
                v1::plan_repository_commit_response::Outcome::Refused(v1::OperationDetail {
                    detail,
                })
            }
        };

        Ok(Response::new(v1::PlanRepositoryCommitResponse {
            outcome: Some(outcome),
        }))
    }

    async fn execute_repository_commit(
        &self,
        request: Request<v1::ExecuteRepositoryCommitRequest>,
    ) -> Result<Response<v1::ExecuteRepositoryCommitResponse>, Status> {
        let request = request.into_inner();
        if request.message.trim().is_empty() {
            return Err(Status::invalid_argument("message must not be empty"));
        }
        let target = application_commit_target(required(request.target, "target")?)?;
        let state = self.state.clone();
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

    async fn change_repository_branch(
        &self,
        request: Request<v1::ChangeRepositoryBranchRequest>,
    ) -> Result<Response<v1::ChangeRepositoryBranchResponse>, Status> {
        let request = request.into_inner();
        let repo_path = absolute_path(request.repository_path, "repository_path")?;
        let onto = BranchName::try_new(request.onto_branch)
            .map_err(|_| Status::invalid_argument("onto_branch must not be empty"))?;
        let action = repository_branch_action(request.action)?;
        let state = self.state.clone();
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

        Ok(Response::new(change_branch_response(result)))
    }

    async fn plan_repository_prune(
        &self,
        request: Request<v1::PlanRepositoryPruneRequest>,
    ) -> Result<Response<v1::PlanRepositoryPruneResponse>, Status> {
        let request = request.into_inner();
        let request = PlanPrune {
            repo_path: absolute_path(request.repository_path, "repository_path")?,
            onto: BranchName::try_new(request.onto_branch)
                .map_err(|_| Status::invalid_argument("onto_branch must not be empty"))?,
        };
        let state = self.state.clone();
        let result = run_blocking(move || plan_prune::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "plan repository branch prune"))?;
        let outcome = match result {
            PlanPruneOk::Ready { top, branches, .. } => {
                v1::plan_repository_prune_response::Outcome::Ready(v1::RepositoryPrunePlan {
                    repository_root: top.to_string(),
                    branches: branches.iter().map(prune_branch).collect(),
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

    async fn execute_repository_prune(
        &self,
        request: Request<v1::ExecuteRepositoryPruneRequest>,
    ) -> Result<Response<v1::ExecuteRepositoryPruneResponse>, Status> {
        let plan = required(request.into_inner().plan, "plan")?;
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
                .map(application_prune_branch)
                .collect::<Result<Vec<_>, _>>()?,
        };
        let state = self.state.clone();
        let result = run_blocking(move || apply_prune::execute(command, &state.git)).await?;

        Ok(Response::new(match result {
            Ok(result) => applied_prune_response(result, None),
            Err(error) => aborted_prune_response(error)?,
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

fn repository_branch_action(raw: i32) -> Result<ChangeBranchAction, Status> {
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

fn change_branch_response(result: ChangeBranchOk) -> v1::ChangeRepositoryBranchResponse {
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

fn aborted_prune_response(
    error: ApplyPruneError,
) -> Result<v1::ExecuteRepositoryPruneResponse, Status> {
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
            Some(result) => applied_prune_response(*result, Some(detail)),
        }),
        _ => Err(Status::internal("execute repository branch prune failed")),
    }
}

fn applied_prune_response(
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
        deleted: result.deleted.iter().map(prune_branch).collect(),
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

fn prune_branch(branch: &PruneBranch) -> v1::PruneBranch {
    v1::PruneBranch {
        name: branch.name.to_string(),
        commit_id: branch.id.to_string(),
    }
}

fn application_prune_branch(branch: v1::PruneBranch) -> Result<PruneBranch, Status> {
    Ok(PruneBranch {
        name: BranchName::try_new(branch.name)
            .map_err(|_| Status::invalid_argument("plan.branches.name must not be empty"))?,
        id: CommitId::try_from(branch.commit_id)
            .map_err(|_| Status::invalid_argument("plan.branches.commit_id is invalid"))?,
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
    results.iter().map(status_result).collect()
}

fn status_result(result: &StatusResult) -> v1::RepositoryStatusResult {
    let state = match result.repository() {
        RepositoryStatus::Absent => {
            v1::repository_status_result::State::Absent(v1::RepositoryAbsentStatus {})
        }
        RepositoryStatus::Present { head, changes } => {
            v1::repository_status_result::State::Present(v1::RepositoryPresentStatus {
                head: Some(status_head(head)),
                changes: Some(status_changes(*changes)),
            })
        }
    };
    v1::RepositoryStatusResult {
        project_name: result.name().to_string(),
        state: Some(state),
    }
}

fn status_head(head: &StatusHead) -> v1::RepositoryStatusHead {
    let state = match head {
        StatusHead::Unavailable => {
            v1::repository_status_head::State::Unavailable(v1::RepositoryStatusUnavailable {})
        }
        StatusHead::Detached => {
            v1::repository_status_head::State::Detached(v1::RepositoryStatusDetached {})
        }
        StatusHead::Branch { name, upstream } => {
            v1::repository_status_head::State::Branch(v1::RepositoryStatusBranch {
                name: name.to_string(),
                upstream: Some(match upstream {
                    StatusUpstream::Missing => v1::repository_status_branch::Upstream::Missing(
                        v1::RepositoryUpstreamMissing {},
                    ),
                    StatusUpstream::Tracking { reference, ahead } => {
                        v1::repository_status_branch::Upstream::Tracking(
                            v1::RepositoryUpstreamTracking {
                                reference: reference.to_string(),
                                commits_ahead: ahead.into_inner(),
                            },
                        )
                    }
                }),
            })
        }
    };
    v1::RepositoryStatusHead { state: Some(state) }
}

fn status_changes(changes: StatusChanges) -> v1::RepositoryStatusChanges {
    let state = match changes {
        StatusChanges::Clean => {
            v1::repository_status_changes::State::Clean(v1::RepositoryChangesClean {})
        }
        StatusChanges::Changed { tracked, untracked } => {
            v1::repository_status_changes::State::Changed(v1::RepositoryChangesChanged {
                tracked_paths: tracked.value(),
                untracked_paths: untracked.value(),
            })
        }
        StatusChanges::Unavailable => {
            v1::repository_status_changes::State::Unavailable(v1::RepositoryChangesUnavailable {})
        }
    };
    v1::RepositoryStatusChanges { state: Some(state) }
}
