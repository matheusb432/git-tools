use gtl_models::projects::catalogue::{Project, ProjectId};
use rusqlite::Connection;

use super::{PROJECT_SELECT, ProjectCatalogueError, read_project};

#[cqrsy::query]
pub fn execute(id: &ProjectId, connection: &Connection) -> Result<Project, ProjectCatalogueError> {
    let mut statement = connection.prepare_cached(&format!(
        "{PROJECT_SELECT} WHERE p.id = ?1 AND p.unmanaged_at IS NULL"
    ))?;
    let mut rows = statement.query([id.as_ref()])?;
    let row = rows.next()?.ok_or(ProjectCatalogueError::NotFound)?;
    read_project(row, connection)
}
