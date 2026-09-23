use gtl_application::{
    ports::{
        ProjectCatalogueDataError, ProjectCatalogueUnavailableError, ProjectClient,
        ProjectClientError,
    },
    projects::catalogue::{ProjectCatalogueError, list_active_projects},
};
use gtl_models::{paths::ProjectName, projects::ProjectRepository};

use crate::app_state::SqliteAppState;

#[derive(Clone, Debug)]
pub struct ProjectRepositoryClient {
    database: SqliteAppState,
}

impl ProjectRepositoryClient {
    #[must_use]
    pub fn new(database: SqliteAppState) -> Self {
        Self { database }
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        let client = self.clone();
        tokio::task::spawn_blocking(move || client.list_repositories())
            .await
            .map_err(|error| ProjectCatalogueUnavailableError::Dependency(error.into()))?
    }

    fn list_repositories(&self) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        let connection = self
            .database
            .connection_lock()
            .map_err(ProjectCatalogueUnavailableError::Dependency)?;
        let projects =
            list_active_projects::execute((), &connection).map_err(|error| match error {
                error @ ProjectCatalogueError::InvalidData(_) => ProjectClientError::InvalidData(
                    ProjectCatalogueDataError::Dependency(error.into()),
                ),
                error => ProjectClientError::Unavailable(
                    ProjectCatalogueUnavailableError::Dependency(error.into()),
                ),
            })?;
        projects
            .into_iter()
            .map(|project| {
                let name = ProjectName::try_new(project.metadata.title.to_string())
                    .map_err(ProjectCatalogueDataError::from)?;
                let path = project
                    .metadata
                    .source
                    .resolve()
                    .map_err(ProjectCatalogueDataError::from)?;
                Ok(ProjectRepository {
                    name,
                    path,
                    remote: project.metadata.git_remote,
                })
            })
            .collect()
    }
}

impl ProjectClient for ProjectRepositoryClient {
    async fn list_projects(&self) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        Self::list_projects(self).await
    }
}
