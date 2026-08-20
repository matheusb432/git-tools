use gtl_application::repositories::{
    get_recursive_repository_statuses::{
        self, GetRecursiveRepositoryStatuses, GetRecursiveRepositoryStatusesError,
    },
    get_repository_status::{self, GetRepositoryStatus},
    resolve_repository_root::ResolveRepositoryRootError,
};
use gtl_models::repository::{
    status::{RepositoryStatus, StatusChanges, StatusHead, StatusResult, StatusUpstream},
    traversal::RepositoryTraversalScope,
};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, run_blocking, unexpected};
use crate::state::AppState;

pub(super) async fn get(
    state: AppState,
    request: v1::GetRepositoryStatusRequest,
) -> Result<Response<v1::RepositoryStatusesResponse>, Status> {
    let request = GetRepositoryStatus {
        repo_path: absolute_path(request.repository_path, "repository_path")?,
    };
    let result = run_blocking(move || get_repository_status::execute(request, &state.git))
        .await?
        .map_err(repository_status_error)?;

    Ok(Response::new(statuses_response(std::slice::from_ref(
        &result,
    ))))
}

pub(super) async fn get_recursive(
    state: AppState,
    request: v1::GetRecursiveRepositoryStatusesRequest,
) -> Result<Response<v1::RepositoryStatusesResponse>, Status> {
    let request = GetRecursiveRepositoryStatuses {
        root: absolute_path(request.root, "root")?,
        scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
    };
    let results =
        run_blocking(move || get_recursive_repository_statuses::execute(request, &state.git))
            .await?
            .map_err(recursive_repository_statuses_error)?;

    Ok(Response::new(statuses_response(&results)))
}

fn repository_status_error(error: ResolveRepositoryRootError) -> Status {
    match error {
        ResolveRepositoryRootError::Rejected { detail, .. } => Status::failed_precondition(detail),
        error => unexpected(error, "get repository status"),
    }
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

pub(crate) fn statuses_response(results: &[StatusResult]) -> v1::RepositoryStatusesResponse {
    v1::RepositoryStatusesResponse {
        results: results.iter().map(status_result).collect(),
    }
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
