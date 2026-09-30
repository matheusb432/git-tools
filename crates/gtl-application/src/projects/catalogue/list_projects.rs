use gtl_models::projects::catalogue::{PROJECTS_MAX, Project, ProjectStatusFilter};
use rusqlite::Connection;

use super::{PROJECT_SELECT, ProjectCatalogueError, read_project};

#[cqrsy::query]
pub fn execute(
    filter: ProjectStatusFilter,
    connection: &Connection,
) -> Result<Vec<Project>, ProjectCatalogueError> {
    let status = match filter {
        ProjectStatusFilter::Active => "AND p.paused_at IS NULL",
        ProjectStatusFilter::Paused => "AND p.paused_at IS NOT NULL",
        ProjectStatusFilter::All => "",
    };
    let mut statement = connection.prepare_cached(&format!(
        "{PROJECT_SELECT} WHERE p.unmanaged_at IS NULL {status} ORDER BY p.id LIMIT ?1"
    ))?;
    let mut rows = statement.query([PROJECTS_MAX + 1])?;
    let mut projects = Vec::new();
    while let Some(row) = rows.next()? {
        if projects.len() == usize::from(PROJECTS_MAX) {
            return Err(ProjectCatalogueError::LimitExceeded);
        }
        projects.push(read_project(row, connection)?);
    }
    Ok(projects)
}
