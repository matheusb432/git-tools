use std::collections::BTreeSet;

use gtl_models::projects::catalogue::{
    ProjectId, ProjectIds, ProjectMutation, ProjectMutationOutcome, ProjectOperationMode,
};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior, params_from_iter};

use super::ProjectCatalogueError;
use crate::history::associate_render_projects;

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
    let changed_ids: BTreeSet<ProjectId> = if request.mode == ProjectOperationMode::Apply {
        let update = match request.membership {
            ProjectMembership::Managed => "SET unmanaged_at = NULL WHERE unmanaged_at IS NOT NULL",
            ProjectMembership::Unmanaged => {
                "SET unmanaged_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE unmanaged_at IS NULL"
            }
        };
        let placeholders = vec!["?"; request.ids.as_slice().len()].join(", ");
        let mut statement = transaction.prepare_cached(&format!(
            "UPDATE projects {update} AND id IN ({placeholders}) RETURNING id"
        ))?;
        let mut rows = statement.query(params_from_iter(
            request.ids.as_slice().iter().map(AsRef::as_ref),
        ))?;
        let mut ids = BTreeSet::new();
        while let Some(row) = rows.next()? {
            let id = row
                .get::<_, String>(0)?
                .try_into()
                .map_err(|error| ProjectCatalogueError::InvalidData(anyhow::Error::new(error)))?;
            ids.insert(id);
        }
        ids
    } else {
        BTreeSet::new()
    };
    let mut mutations = Vec::new();
    for id in request.ids.as_slice() {
        let changed = changed_ids.contains(id) || {
            let managed: bool = transaction
                .prepare_cached("SELECT unmanaged_at IS NULL FROM projects WHERE id = ?1")?
                .query_one([id.as_ref()], |row| row.get(0))
                .optional()?
                .ok_or(ProjectCatalogueError::NotFound)?;
            managed != matches!(request.membership, ProjectMembership::Managed)
        };
        mutations.push(ProjectMutation {
            id: id.clone(),
            outcome: if changed {
                ProjectMutationOutcome::Changed
            } else {
                ProjectMutationOutcome::Unchanged
            },
        });
    }
    if matches!(request.membership, ProjectMembership::Managed)
        && request.mode == ProjectOperationMode::Apply
    {
        associate_render_projects::execute((), &transaction)
            .map_err(ProjectCatalogueError::InvalidData)?;
    }
    transaction.commit()?;
    Ok(mutations)
}
