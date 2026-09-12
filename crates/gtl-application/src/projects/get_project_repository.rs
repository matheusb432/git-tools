use std::path::PathBuf;

use gtl_models::{paths::RepositoryRoot, projects::catalogue::ProjectId};
use rusqlite::Connection;

use super::catalogue::{ProjectCatalogueError, get_project};

pub struct GetProjectRepository {
    pub id: ProjectId,
    pub home: PathBuf,
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
        .resolve(&request.home)
        .map_err(|error| ProjectCatalogueError::InvalidData(error.into()))
}
