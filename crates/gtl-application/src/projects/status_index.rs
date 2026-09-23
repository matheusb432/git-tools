use gtl_models::repository::status::{RepositoryStatus, StatusChanges, StatusHead, StatusUpstream};
use gtl_wire::viewer::projects::{
    ViewerProject, ViewerProjectBranchComparison, ViewerProjectStatus,
};
use rusqlite::{Connection, params};

pub fn record(
    project: &ViewerProject,
    status: Option<&ViewerProjectStatus>,
    connection: &Connection,
) -> anyhow::Result<()> {
    if !super::list_viewer_projects::get_project(&project.id, connection)?.is_some_and(|current| {
        current.path == project.path && current.comparison_branch == project.comparison_branch
    }) {
        return Ok(());
    }
    if status.is_none()
        && connection.execute(
            "UPDATE project_status_index SET checked_at = unixepoch()
         WHERE project_id = ?1 AND comparison_branch = ?2
           AND source_id = (SELECT source_id FROM projects WHERE id = ?1)",
            params![project.id.as_ref(), project.comparison_branch.as_ref()],
        )? > 0
    {
        return Ok(());
    }
    let (branch, tracked, untracked) = match status.map(|status| &status.status) {
        Some(RepositoryStatus::Present { head, changes }) => {
            let branch = match head {
                StatusHead::Branch { name, .. } => Some(name.as_str()),
                StatusHead::Detached => Some("Detached HEAD"),
                StatusHead::Unavailable => None,
            };
            let changes = match changes {
                StatusChanges::Clean => (Some(false), Some(false)),
                StatusChanges::Changed { tracked, untracked } => {
                    (Some(!tracked.is_zero()), Some(!untracked.is_zero()))
                }
                StatusChanges::Unavailable => (None, None),
            };
            (branch, changes.0, changes.1)
        }
        Some(RepositoryStatus::Absent) | None => (None, None, None),
    };
    let ahead = status
        .and_then(|status| match &status.branch_comparison {
            ViewerProjectBranchComparison::Branch { commits_ahead } => {
                Some(commits_ahead.into_inner())
            }
            ViewerProjectBranchComparison::Upstream => match &status.status {
                RepositoryStatus::Present {
                    head:
                        StatusHead::Branch {
                            upstream: StatusUpstream::Tracking { ahead, .. },
                            ..
                        },
                    ..
                } => Some(ahead.into_inner()),
                _ => None,
            },
            ViewerProjectBranchComparison::Unavailable { .. } => None,
        })
        .map(i64::try_from)
        .transpose()?;
    connection.execute(
        "INSERT INTO project_status_index
            (project_id, source_id, comparison_branch, branch, commits_ahead, tracked_changes, untracked_changes, checked_at)
         SELECT id, source_id, comparison_branch, ?2, ?3, ?4, ?5, unixepoch()
         FROM projects WHERE id = ?1 AND comparison_branch = ?6
         ON CONFLICT(project_id) DO UPDATE SET
            source_id = excluded.source_id, comparison_branch = excluded.comparison_branch,
            branch = excluded.branch, commits_ahead = excluded.commits_ahead,
            tracked_changes = excluded.tracked_changes, untracked_changes = excluded.untracked_changes,
            checked_at = excluded.checked_at",
        params![project.id.as_ref(), branch, ahead, tracked, untracked, project.comparison_branch.as_ref()],
    )?;
    Ok(())
}
