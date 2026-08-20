use anyhow::Context as _;
use gtl_application::{
    repositories::{find_repositories, get_repository_statuses, resolve_repository_root},
    shared::repository_name,
};
use gtl_models::repository::{
    status::{RepositoryStatus, StatusChanges, StatusHead, StatusResult, StatusUpstream},
    traversal::{RepositoryTarget, RepositoryTraversalScope},
};
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{absolute_path, run_blocking, unexpected};
use crate::state::AppState;

pub(super) async fn get(
    state: AppState,
    request: v1::GetRepositoryStatusRequest,
) -> Result<Response<v1::RepositoryStatusesResponse>, Status> {
    let repository_path = absolute_path(request.repository_path, "repository_path")?;
    let results = run_blocking(move || {
        let root = resolve_repository_root::execute(
            resolve_repository_root::ResolveRepositoryRoot {
                repo_path: repository_path,
            },
            &state.git,
        )?;
        Ok::<_, anyhow::Error>(get_repository_statuses::execute(
            get_repository_statuses::GetRepositoryStatuses {
                repos: vec![RepositoryTarget {
                    label: repository_name::from_root(&root),
                    path: root,
                }],
            },
            &state.git,
        ))
    })
    .await?
    .map_err(|error| unexpected(error, "get repository status"))?;

    Ok(Response::new(statuses_response(&results)))
}

pub(super) async fn get_recursive(
    state: AppState,
    request: v1::GetRecursiveRepositoryStatusesRequest,
) -> Result<Response<v1::RepositoryStatusesResponse>, Status> {
    let root = absolute_path(request.root, "root")?;
    let results = run_blocking(move || {
        let root = std::fs::canonicalize(&root)
            .with_context(|| format!("resolving repository traversal root {}", root.display()))?;
        let repos = find_repositories::execute(find_repositories::FindRepositories {
            root: root.clone(),
            scope: RepositoryTraversalScope::ExcludeLinkedWorktrees,
        })?;
        if repos.is_empty() {
            anyhow::bail!("no git repos found under {}", root.display());
        }
        Ok::<_, anyhow::Error>(get_repository_statuses::execute(
            get_repository_statuses::GetRepositoryStatuses { repos },
            &state.git,
        ))
    })
    .await?
    .map_err(|error| unexpected(error, "get recursive repository statuses"))?;

    Ok(Response::new(statuses_response(&results)))
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
