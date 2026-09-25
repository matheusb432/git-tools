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
    let paused: bool = transaction
        .query_row(
            "SELECT paused_at IS NOT NULL FROM projects WHERE id = ?1 AND unmanaged_at IS NULL",
            [request.id.as_ref()],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(ProjectCatalogueError::NotFound)?;
    let changed = paused != (request.status == ProjectStatus::Paused);
    if changed && request.mode == ProjectOperationMode::Apply {
        let sql = match request.status {
            ProjectStatus::Active => "UPDATE projects SET paused_at = NULL WHERE id = ?1",
            ProjectStatus::Paused => {
                "UPDATE projects SET paused_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1"
            }
        };
        transaction.execute(sql, [request.id.as_ref()])?;
    }
    transaction.commit()?;
    Ok(SetProjectStatusOk {
        target_status: request.status,
        outcome: if changed {
            ProjectMutationOutcome::Changed
        } else {
            ProjectMutationOutcome::Unchanged
        },
    })
}
