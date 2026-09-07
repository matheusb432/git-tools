use gtl_models::{
    git::CommitCount,
    live_views::LiveComparison,
    repository::{
        PathCount,
        status::{RepositoryStatus, StatusChanges, StatusHead, StatusResult, StatusUpstream},
    },
};

use super::{ViewerCodecError, required};
use crate::{
    v1,
    viewer::projects::{
        OpenViewerProject, OpenViewerProjectOk, VIEWER_PROJECTS_MAX, ViewerProject,
    },
};

#[must_use]
pub fn encode_project(project: ViewerProject) -> v1::ViewerProject {
    let status = match project.status {
        RepositoryStatus::Absent => StatusResult::absent(project.name),
        RepositoryStatus::Present { head, changes } => {
            StatusResult::present(project.name, head, changes)
        }
    };
    v1::ViewerProject {
        path: project.path.to_string(),
        status: Some(encode_status_result(&status)),
        last_rendered_at: project
            .last_rendered_at
            .map(|timestamp| timestamp.to_string()),
    }
}

pub fn decode_projects(
    response: v1::ListViewerProjectsResponse,
) -> Result<Vec<ViewerProject>, ViewerCodecError> {
    if response.projects.len() > VIEWER_PROJECTS_MAX {
        return Err(ViewerCodecError::InvalidMessage);
    }
    response
        .projects
        .into_iter()
        .map(|project| {
            let status = required(project.status)?;
            let name = status
                .project_name
                .try_into()
                .map_err(|_| ViewerCodecError::InvalidMessage)?;
            let status = match required(status.state)? {
                v1::repository_status_result::State::Absent(_) => RepositoryStatus::Absent,
                v1::repository_status_result::State::Present(present) => {
                    RepositoryStatus::Present {
                        head: decode_head(required(present.head)?)?,
                        changes: decode_changes(required(present.changes)?)?,
                    }
                }
            };
            Ok(ViewerProject {
                path: gtl_models::paths::RepositoryRoot::try_new(project.path.into())
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
                name,
                status,
                last_rendered_at: project
                    .last_rendered_at
                    .map(TryInto::try_into)
                    .transpose()
                    .map_err(|_| ViewerCodecError::InvalidMessage)?,
            })
        })
        .collect()
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
        comparison: match request.comparison {
            LiveComparison::LocalChanges => v1::ViewerProjectComparison::LocalChanges,
            LiveComparison::UnpushedCommits => v1::ViewerProjectComparison::UnpushedCommits,
        }
        .into(),
    }
}

pub fn decode_open(
    request: v1::OpenViewerProjectRequest,
) -> Result<OpenViewerProject, ViewerCodecError> {
    let comparison = match v1::ViewerProjectComparison::try_from(request.comparison) {
        Ok(v1::ViewerProjectComparison::LocalChanges) => LiveComparison::LocalChanges,
        Ok(v1::ViewerProjectComparison::UnpushedCommits) => LiveComparison::UnpushedCommits,
        _ => return Err(ViewerCodecError::InvalidMessage),
    };
    Ok(OpenViewerProject {
        path: gtl_models::paths::RepositoryRoot::try_new(request.path.into())
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        comparison,
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
