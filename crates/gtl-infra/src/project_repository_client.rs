use gtl_application::{
    ports::{
        ProjectCatalogueDataError, ProjectCatalogueUnavailableError, ProjectClient,
        ProjectClientError,
    },
    projects::{
        catalogue::{ProjectCatalogueError, list_projects},
        find_project_by_repository,
    },
    viewer::push::ViewerPushProject,
};
use gtl_models::{
    paths::{ProjectName, RepositoryRoot},
    projects::{ProjectRepository, catalogue::ProjectStatusFilter},
};

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
        self.list_repositories(ProjectStatusFilter::Active).await
    }

    /// Lists active and paused projects for operations that name one project explicitly.
    pub async fn list_managed_projects(
        &self,
    ) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        self.list_repositories(ProjectStatusFilter::All).await
    }

    async fn list_repositories(
        &self,
        filter: ProjectStatusFilter,
    ) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        let client = self.clone();
        tokio::task::spawn_blocking(move || client.read_repositories(filter))
            .await
            .map_err(|error| ProjectCatalogueUnavailableError::Dependency(error.into()))?
    }

    fn read_repositories(
        &self,
        filter: ProjectStatusFilter,
    ) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        let connection = self
            .database
            .connection_lock()
            .map_err(ProjectCatalogueUnavailableError::Dependency)?;
        let projects =
            list_projects::execute(filter, &connection).map_err(|error| match error {
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

impl ViewerPushProject for ProjectRepositoryClient {
    fn project_name(&self, path: &RepositoryRoot) -> anyhow::Result<Option<ProjectName>> {
        let connection = self.database.connection_lock()?;
        find_project_by_repository::project_name(path, &connection)
    }
}
