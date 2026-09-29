use gtl_models::projects::catalogue::{
    ProjectId, ProjectMutationOutcome, ProjectOperationMode, ProjectStatus,
};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior};

use super::catalogue::ProjectCatalogueError;

pub struct SetProjectStatus {
    pub id: ProjectId,
    pub status: ProjectStatus,
    pub mode: ProjectOperationMode,
}

pub struct SetProjectStatusOk {
    pub target_status: ProjectStatus,
    pub outcome: ProjectMutationOutcome,
}

#[cqrsy::command]
pub fn execute(
    request: &SetProjectStatus,
    connection: &mut Connection,
) -> Result<SetProjectStatusOk, ProjectCatalogueError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let applied_status = if request.mode == ProjectOperationMode::Apply {
        let sql = match request.status {
            ProjectStatus::Active => {
                "UPDATE projects SET paused_at = NULL
                 WHERE id = ?1 AND unmanaged_at IS NULL AND paused_at IS NOT NULL
                 RETURNING paused_at IS NOT NULL"
            }
            ProjectStatus::Paused => {
                "UPDATE projects SET paused_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1 AND unmanaged_at IS NULL AND paused_at IS NULL
                 RETURNING paused_at IS NOT NULL"
            }
        };
        transaction
            .prepare_cached(sql)?
            .query_one([request.id.as_ref()], |row| row.get::<_, bool>(0))
            .optional()?
            .map(|paused| {
                if paused {
                    ProjectStatus::Paused
                } else {
                    ProjectStatus::Active
                }
            })
    } else {
        None
    };
    let changed = if applied_status.is_some() {
        true
    } else {
        let paused: bool = transaction
            .prepare_cached(
                "SELECT paused_at IS NOT NULL FROM projects WHERE id = ?1 AND unmanaged_at IS NULL",
            )?
            .query_one([request.id.as_ref()], |row| row.get(0))
            .optional()?
            .ok_or(ProjectCatalogueError::NotFound)?;
        paused != (request.status == ProjectStatus::Paused)
    };
    transaction.commit()?;
    Ok(SetProjectStatusOk {
        target_status: applied_status.unwrap_or(request.status),
        outcome: if changed {
            ProjectMutationOutcome::Changed
        } else {
            ProjectMutationOutcome::Unchanged
        },
    })
}
