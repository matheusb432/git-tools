use gtl_models::{paths::RepositoryRoot, projects::catalogue::ProjectId};
use rusqlite::Connection;

use super::catalogue::{ProjectCatalogueError, get_project};

pub struct GetProjectRepository {
    pub id: ProjectId,
}

#[cqrsy::query]
pub fn execute(
    request: &GetProjectRepository,
    connection: &Connection,
) -> Result<RepositoryRoot, ProjectCatalogueError> {
    let project = get_project::execute(&request.id, connection)?;
    project
        .metadata
        .source
        .resolve()
        .map_err(|error| ProjectCatalogueError::InvalidData(error.into()))
}
