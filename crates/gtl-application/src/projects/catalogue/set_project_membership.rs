use gtl_models::projects::catalogue::{
    ProjectIds, ProjectMutation, ProjectMutationOutcome, ProjectOperationMode,
};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior};

use super::ProjectCatalogueError;

pub enum ProjectMembership {
    Managed,
    Unmanaged,
}

pub struct SetProjectMembership {
    pub ids: ProjectIds,
    pub membership: ProjectMembership,
    pub mode: ProjectOperationMode,
}

#[cqrsy::command]
pub fn execute(
    request: &SetProjectMembership,
    connection: &mut Connection,
) -> Result<Vec<ProjectMutation>, ProjectCatalogueError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut mutations = Vec::new();
    for id in request.ids.as_slice() {
        let managed: bool = transaction
            .query_row(
                "SELECT unmanaged_at IS NULL FROM projects WHERE id = ?1",
                [id.as_ref()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(ProjectCatalogueError::NotFound)?;
        let changed = managed != matches!(request.membership, ProjectMembership::Managed);
        if changed && request.mode == ProjectOperationMode::Apply {
            let sql = match request.membership {
                ProjectMembership::Managed => {
                    "UPDATE projects SET unmanaged_at = NULL WHERE id = ?1"
                }
                ProjectMembership::Unmanaged => {
                    "UPDATE projects SET unmanaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?1"
                }
            };
            transaction.execute(sql, [id.as_ref()])?;
        }
        mutations.push(ProjectMutation {
            id: id.clone(),
            outcome: if changed {
                ProjectMutationOutcome::Changed
            } else {
                ProjectMutationOutcome::Unchanged
            },
        });
    }
    transaction.commit()?;
    Ok(mutations)
}
