use gtl_models::{
    git::CommitCount,
    repository::{
        PathCount,
        status::{RepositoryStatus, StatusChanges, StatusHead, StatusResult, StatusUpstream},
    },
};

use super::{ViewerCodecError, required};
use crate::{
    v1,
    viewer::projects::{
        DiscoverProjectRepositories, DiscoveredProjectRepository, GetViewerProjectStatus,
        ImportProjectRepositories, ListViewerProjects, OpenViewerProject, OpenViewerProjectOk,
        ProjectDiscovery, ProjectDiscoveryState, ProjectImportOutcome, ProjectImportResult,
        ProjectImportSelection, UpdateViewerProject, ViewerProject, ViewerProjectBranchComparison,
        ViewerProjectDiffMode, ViewerProjectPage, ViewerProjectStatus, ViewerProjectsCursor,
        ViewerProjectsPageSize,
    },
};

#[must_use]
pub fn encode_discover(
    request: DiscoverProjectRepositories,
) -> v1::DiscoverProjectRepositoriesRequest {
    v1::DiscoverProjectRepositoriesRequest { root: request.root }
}

pub fn decode_discover(
    request: v1::DiscoverProjectRepositoriesRequest,
) -> Result<DiscoverProjectRepositories, ViewerCodecError> {
    if request.root.trim().is_empty() {
        return Err(ViewerCodecError::InvalidMessage);
    }
    Ok(DiscoverProjectRepositories { root: request.root })
}

#[must_use]
pub fn encode_discovery(discovery: ProjectDiscovery) -> v1::DiscoverProjectRepositoriesResponse {
    v1::DiscoverProjectRepositoriesResponse {
        root: discovery.root,
        repositories: discovery
            .repositories
            .into_iter()
            .map(|repository| {
                let (state, existing_project_id) = match repository.state {
                    ProjectDiscoveryState::New => (v1::ProjectDiscoveryState::New, None),
                    ProjectDiscoveryState::Active(id) => {
                        (v1::ProjectDiscoveryState::Active, Some(id.to_string()))
                    }
                    ProjectDiscoveryState::Paused(id) => {
                        (v1::ProjectDiscoveryState::Paused, Some(id.to_string()))
                    }
                    ProjectDiscoveryState::Unmanaged(id) => {
                        (v1::ProjectDiscoveryState::Unmanaged, Some(id.to_string()))
                    }
                };
                v1::DiscoveredProjectRepository {
                    path: repository.path.to_string(),
                    label: repository.label.to_string(),
                    state: state as i32,
                    existing_project_id,
                }
            })
            .collect(),
    }
}

pub fn decode_discovery(
    response: v1::DiscoverProjectRepositoriesResponse,
) -> Result<ProjectDiscovery, ViewerCodecError> {
    if response.repositories.len() > usize::from(gtl_models::projects::catalogue::PROJECTS_MAX)
        || !std::path::Path::new(&response.root).is_absolute()
    {
        return Err(ViewerCodecError::InvalidMessage);
    }
    let repositories = response
        .repositories
        .into_iter()
        .map(|repository| {
            let id = repository
                .existing_project_id
                .map(|id| id.try_into().map_err(|_| ViewerCodecError::InvalidMessage))
                .transpose()?;
            let state = match (v1::ProjectDiscoveryState::try_from(repository.state), id) {
                (Ok(v1::ProjectDiscoveryState::New), None) => ProjectDiscoveryState::New,
                (Ok(v1::ProjectDiscoveryState::Active), Some(id)) => {
                    ProjectDiscoveryState::Active(id)
                }
                (Ok(v1::ProjectDiscoveryState::Paused), Some(id)) => {
                    ProjectDiscoveryState::Paused(id)
                }
                (Ok(v1::ProjectDiscoveryState::Unmanaged), Some(id)) => {
                    ProjectDiscoveryState::Unmanaged(id)
                }
                _ => return Err(ViewerCodecError::InvalidMessage),
            };
            Ok(DiscoveredProjectRepository {
                path: gtl_models::paths::RepositoryRoot::try_new(repository.path.into())
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
                label: repository
                    .label
                    .try_into()
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
                state,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProjectDiscovery {
        root: response.root,
        repositories,
    })
}

#[must_use]
pub fn encode_import(request: ImportProjectRepositories) -> v1::ImportProjectRepositoriesRequest {
    v1::ImportProjectRepositoriesRequest {
        selections: request
            .selections
            .into_iter()
            .map(|selection| v1::ProjectImportSelection {
                path: selection.path,
                project_id: selection.project_id,
                title: selection.title,
            })
            .collect(),
    }
}

pub fn decode_import(
    request: v1::ImportProjectRepositoriesRequest,
) -> Result<ImportProjectRepositories, ViewerCodecError> {
    if request.selections.is_empty()
        || request.selections.len() > usize::from(gtl_models::projects::catalogue::PROJECTS_MAX)
    {
        return Err(ViewerCodecError::InvalidMessage);
    }
    Ok(ImportProjectRepositories {
        selections: request
            .selections
            .into_iter()
            .map(|selection| ProjectImportSelection {
                path: selection.path,
                project_id: selection.project_id,
                title: selection.title,
            })
            .collect(),
    })
}

#[must_use]
pub fn encode_import_results(
    results: Vec<ProjectImportResult>,
) -> v1::ImportProjectRepositoriesResponse {
    v1::ImportProjectRepositoriesResponse {
        results: results
            .into_iter()
            .map(|result| {
                let (outcome, error) = match result.outcome {
                    ProjectImportOutcome::Created => (v1::ProjectImportOutcome::Created, None),
                    ProjectImportOutcome::Restored => (v1::ProjectImportOutcome::Restored, None),
                    ProjectImportOutcome::Failed(message) => {
                        (v1::ProjectImportOutcome::Failed, Some(message))
                    }
                };
                v1::ProjectImportResult {
                    path: result.path,
                    project_id: result.project_id,
                    outcome: outcome as i32,
                    error,
                }
            })
            .collect(),
    }
}

pub fn decode_import_results(
    response: v1::ImportProjectRepositoriesResponse,
) -> Result<Vec<ProjectImportResult>, ViewerCodecError> {
    if response.results.len() > usize::from(gtl_models::projects::catalogue::PROJECTS_MAX) {
        return Err(ViewerCodecError::InvalidMessage);
    }
    response
        .results
        .into_iter()
        .map(|result| {
            let outcome = match (
                v1::ProjectImportOutcome::try_from(result.outcome),
                result.error,
            ) {
                (Ok(v1::ProjectImportOutcome::Created), None) => ProjectImportOutcome::Created,
                (Ok(v1::ProjectImportOutcome::Restored), None) => ProjectImportOutcome::Restored,
                (Ok(v1::ProjectImportOutcome::Failed), Some(error)) if !error.is_empty() => {
                    ProjectImportOutcome::Failed(error)
                }
                _ => return Err(ViewerCodecError::InvalidMessage),
            };
            Ok(ProjectImportResult {
                path: result.path,
                project_id: result.project_id,
                outcome,
            })
        })
        .collect()
}

#[must_use]
pub fn encode_list(request: ListViewerProjects) -> v1::ListViewerProjectsRequest {
    use v1::list_viewer_projects_request::Cursor;
    v1::ListViewerProjectsRequest {
        sort: request
            .sort
            .map(|sort| super::encode_projects_sort(sort) as i32),
        page_size: request.page_size.into_inner(),
        cursor: Some(match request.cursor {
            ViewerProjectsCursor::First => Cursor::First(v1::Empty {}),
            ViewerProjectsCursor::After(id) => Cursor::AfterProjectId(id.to_string()),
            ViewerProjectsCursor::Before(id) => Cursor::BeforeProjectId(id.to_string()),
            ViewerProjectsCursor::Last => Cursor::Last(v1::Empty {}),
        }),
    }
}

pub fn decode_list(
    request: v1::ListViewerProjectsRequest,
) -> Result<ListViewerProjects, ViewerCodecError> {
    use v1::list_viewer_projects_request::Cursor;
    Ok(ListViewerProjects {
        sort: request.sort.map(super::decode_projects_sort).transpose()?,
        page_size: ViewerProjectsPageSize::try_new(request.page_size)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        cursor: match required(request.cursor)? {
            Cursor::First(_) => ViewerProjectsCursor::First,
            Cursor::Last(_) => ViewerProjectsCursor::Last,
            Cursor::AfterProjectId(id) => ViewerProjectsCursor::After(
                id.try_into()
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
            ),
            Cursor::BeforeProjectId(id) => ViewerProjectsCursor::Before(
                id.try_into()
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
            ),
        },
    })
}

#[must_use]
pub fn encode_project(project: &ViewerProject) -> v1::ViewerProject {
    v1::ViewerProject {
        id: project.id.to_string(),
        name: project.name.to_string(),
        comparison_branch: project.comparison_branch.to_string(),
        path: project.path.to_string(),
        last_rendered_at: project.last_rendered_at.as_ref().map(ToString::to_string),
    }
}

#[must_use]
pub fn encode_page(page: &ViewerProjectPage) -> v1::ListViewerProjectsResponse {
    v1::ListViewerProjectsResponse {
        projects: page.projects().iter().map(encode_project).collect(),
        total: page.total(),
        count_before: page.count_before(),
    }
}

pub fn decode_projects(
    response: v1::ListViewerProjectsResponse,
) -> Result<ViewerProjectPage, ViewerCodecError> {
    if response.projects.len() > 100 {
        return Err(ViewerCodecError::InvalidMessage);
    }
    let projects = response
        .projects
        .into_iter()
        .map(|project| {
            let decode = || -> Result<ViewerProject, Box<dyn std::error::Error>> {
                Ok(ViewerProject {
                    id: project.id.try_into()?,
                    name: project.name.try_into()?,
                    comparison_branch: project.comparison_branch.try_into()?,
                    path: gtl_models::paths::RepositoryRoot::try_new(project.path.into())?,
                    last_rendered_at: project
                        .last_rendered_at
                        .map(TryInto::try_into)
                        .transpose()?,
                })
            };
            decode().map_err(|_| ViewerCodecError::InvalidMessage)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ViewerProjectPage::try_new(projects, response.total, response.count_before)
        .ok_or(ViewerCodecError::InvalidMessage)
}

#[must_use]
pub fn encode_get_status(request: GetViewerProjectStatus) -> v1::GetViewerProjectStatusRequest {
    v1::GetViewerProjectStatusRequest {
        project_id: request.project_id.into_inner(),
    }
}

pub fn decode_get_status(
    request: v1::GetViewerProjectStatusRequest,
) -> Result<GetViewerProjectStatus, ViewerCodecError> {
    Ok(GetViewerProjectStatus {
        project_id: request
            .project_id
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
    })
}

#[must_use]
pub fn encode_project_status(project: ViewerProjectStatus) -> v1::GetViewerProjectStatusResponse {
    use v1::get_viewer_project_status_response::Status;
    v1::GetViewerProjectStatusResponse {
        project_id: project.project_id.to_string(),
        comparison_branch: project.comparison_branch.to_string(),
        branch_comparison: Some(encode_branch_comparison(project.branch_comparison)),
        status: Some(match project.status {
            RepositoryStatus::Absent => Status::Absent(v1::RepositoryAbsentStatus {}),
            RepositoryStatus::Present { head, changes } => {
                Status::Present(v1::RepositoryPresentStatus {
                    head: Some(status_head(&head)),
                    changes: Some(status_changes(changes)),
                })
            }
        }),
    }
}

pub fn decode_project_status(
    response: v1::GetViewerProjectStatusResponse,
) -> Result<ViewerProjectStatus, ViewerCodecError> {
    use v1::get_viewer_project_status_response::Status;
    Ok(ViewerProjectStatus {
        project_id: response
            .project_id
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        comparison_branch: response
            .comparison_branch
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        branch_comparison: decode_branch_comparison(required(response.branch_comparison)?)?,
        status: match required(response.status)? {
            Status::Absent(_) => RepositoryStatus::Absent,
            Status::Present(present) => RepositoryStatus::Present {
                head: decode_head(required(present.head)?)?,
                changes: decode_changes(required(present.changes)?)?,
            },
        },
    })
}

fn decode_head(head: v1::RepositoryStatusHead) -> Result<StatusHead, ViewerCodecError> {
    Ok(match required(head.state)? {
        v1::repository_status_head::State::Unavailable(_) => StatusHead::Unavailable,
        v1::repository_status_head::State::Detached(_) => StatusHead::Detached,
        v1::repository_status_head::State::Branch(branch) => StatusHead::Branch {
            name: branch
                .name
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?,
            upstream: match required(branch.upstream)? {
                v1::repository_status_branch::Upstream::Missing(_) => StatusUpstream::Missing,
                v1::repository_status_branch::Upstream::Tracking(tracking) => {
                    StatusUpstream::Tracking {
                        reference: tracking
                            .reference
                            .try_into()
                            .map_err(|_| ViewerCodecError::InvalidMessage)?,
                        ahead: CommitCount::new(tracking.commits_ahead),
                    }
                }
            },
        },
    })
}

fn decode_changes(changes: v1::RepositoryStatusChanges) -> Result<StatusChanges, ViewerCodecError> {
    Ok(match required(changes.state)? {
        v1::repository_status_changes::State::Clean(_) => StatusChanges::Clean,
        v1::repository_status_changes::State::Unavailable(_) => StatusChanges::Unavailable,
        v1::repository_status_changes::State::Changed(counts) => StatusChanges::from_counts(
            PathCount::new(counts.tracked_paths),
            PathCount::new(counts.untracked_paths),
        ),
    })
}

#[must_use]
pub fn encode_open(request: &OpenViewerProject) -> v1::OpenViewerProjectRequest {
    v1::OpenViewerProjectRequest {
        path: request.path.to_string(),
        mode: match request.mode {
            ViewerProjectDiffMode::Snapshot => v1::ViewerProjectDiffMode::Snapshot,
            ViewerProjectDiffMode::Live => v1::ViewerProjectDiffMode::Live,
        }
        .into(),
    }
}

pub fn decode_open(
    request: v1::OpenViewerProjectRequest,
) -> Result<OpenViewerProject, ViewerCodecError> {
    let mode = match v1::ViewerProjectDiffMode::try_from(request.mode) {
        Ok(v1::ViewerProjectDiffMode::Snapshot) => ViewerProjectDiffMode::Snapshot,
        Ok(v1::ViewerProjectDiffMode::Live) => ViewerProjectDiffMode::Live,
        _ => return Err(ViewerCodecError::InvalidMessage),
    };
    Ok(OpenViewerProject {
        path: gtl_models::paths::RepositoryRoot::try_new(request.path.into())
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        mode,
    })
}

pub fn decode_open_response(
    response: v1::OpenViewerProjectResponse,
) -> Result<OpenViewerProjectOk, ViewerCodecError> {
    Ok(OpenViewerProjectOk {
        tab_id: response
            .tab_id
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
    })
}

#[must_use]
pub fn encode_status_result(result: &StatusResult) -> v1::RepositoryStatusResult {
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

fn encode_branch_comparison(
    comparison: ViewerProjectBranchComparison,
) -> v1::ViewerProjectBranchComparison {
    use v1::viewer_project_branch_comparison::State;

    v1::ViewerProjectBranchComparison {
        state: Some(match comparison {
            ViewerProjectBranchComparison::Upstream => State::Upstream(v1::Empty {}),
            ViewerProjectBranchComparison::Branch { commits_ahead } => {
                State::CommitsAhead(commits_ahead.into_inner())
            }
            ViewerProjectBranchComparison::Unavailable { reason } => {
                State::UnavailableReason(reason)
            }
        }),
    }
}

fn decode_branch_comparison(
    comparison: v1::ViewerProjectBranchComparison,
) -> Result<ViewerProjectBranchComparison, ViewerCodecError> {
    use v1::viewer_project_branch_comparison::State;

    Ok(match required(comparison.state)? {
        State::Upstream(_) => ViewerProjectBranchComparison::Upstream,
        State::CommitsAhead(count) => ViewerProjectBranchComparison::Branch {
            commits_ahead: CommitCount::new(count),
        },
        State::UnavailableReason(reason) => ViewerProjectBranchComparison::Unavailable { reason },
    })
}

#[must_use]
pub fn encode_update(request: UpdateViewerProject) -> v1::UpdateViewerProjectRequest {
    use v1::comparison_branch_field_update::Operation;

    use crate::viewer::FieldUpdate;
    v1::UpdateViewerProjectRequest {
        path: request.path.to_string(),
        expected_comparison_branch: request.expected_comparison_branch.to_string(),
        comparison_branch: match request.comparison_branch {
            FieldUpdate::Unchanged => None,
            FieldUpdate::Clear => Some(v1::ComparisonBranchFieldUpdate {
                operation: Some(Operation::Clear(v1::ClearSetting {})),
            }),
            FieldUpdate::Update(branch) => Some(v1::ComparisonBranchFieldUpdate {
                operation: Some(Operation::Update(branch.to_string())),
            }),
        },
    }
}

pub fn decode_update(
    request: v1::UpdateViewerProjectRequest,
) -> Result<UpdateViewerProject, ViewerCodecError> {
    use v1::comparison_branch_field_update::Operation;

    use crate::viewer::FieldUpdate;
    Ok(UpdateViewerProject {
        path: gtl_models::paths::RepositoryRoot::try_new(request.path.into())
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        expected_comparison_branch: request
            .expected_comparison_branch
            .try_into()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        comparison_branch: match request.comparison_branch {
            None => FieldUpdate::Unchanged,
            Some(field) => match required(field.operation)? {
                Operation::Clear(_) => FieldUpdate::Clear,
                Operation::Update(branch) => FieldUpdate::Update(
                    branch
                        .try_into()
                        .map_err(|_| ViewerCodecError::InvalidMessage)?,
                ),
            },
        },
    })
}

pub fn decode_status_update(
    update: v1::ViewerProjectStatusUpdate,
) -> Result<crate::viewer::projects::ViewerProjectStatusUpdate, ViewerCodecError> {
    use crate::viewer::projects::ViewerProjectStatusUpdate;
    let id = update
        .project_id
        .try_into()
        .map_err(|_| ViewerCodecError::InvalidMessage)?;
    match required(update.result)? {
        v1::viewer_project_status_update::Result::Status(status) => {
            let status = decode_project_status(status)?;
            if status.project_id != id {
                return Err(ViewerCodecError::InvalidMessage);
            }
            Ok(ViewerProjectStatusUpdate::Status(status))
        }
        v1::viewer_project_status_update::Result::Unavailable(_) => {
            Ok(ViewerProjectStatusUpdate::Unavailable(id))
        }
    }
}

#[must_use]
pub fn encode_status_update(
    update: crate::viewer::projects::ViewerProjectStatusUpdate,
) -> v1::ViewerProjectStatusUpdate {
    use crate::viewer::projects::ViewerProjectStatusUpdate;
    match update {
        ViewerProjectStatusUpdate::Status(status) => v1::ViewerProjectStatusUpdate {
            project_id: status.project_id.to_string(),
            result: Some(v1::viewer_project_status_update::Result::Status(
                encode_project_status(status),
            )),
        },
        ViewerProjectStatusUpdate::Unavailable(id) => v1::ViewerProjectStatusUpdate {
            project_id: id.to_string(),
            result: Some(v1::viewer_project_status_update::Result::Unavailable(
                v1::Empty {},
            )),
        },
    }
}
