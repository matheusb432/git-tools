use std::future::Future;

use gtl_models::{
    git::RemoteUrlError,
    paths::{ProjectNameError, RepositoryRootError},
    projects::ProjectRepository,
};

#[derive(Debug, thiserror::Error)]
pub enum ProjectClientError {
    #[error(transparent)]
    InvalidConfiguration(#[from] ProjectCatalogueConfigurationError),
    #[error(transparent)]
    Unavailable(#[from] ProjectCatalogueUnavailableError),
    #[error(transparent)]
    InvalidData(#[from] ProjectCatalogueDataError),
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectCatalogueConfigurationError {
    #[error("project catalogue home directory is unavailable")]
    HomeDirectoryUnavailable,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectCatalogueUnavailableError {
    #[error("project catalogue is temporarily unavailable")]
    Dependency(#[source] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectCatalogueDataError {
    #[error("project catalogue returned invalid data")]
    Dependency(#[source] anyhow::Error),
    #[error("project catalogue returned an invalid project name")]
    ProjectName(#[from] ProjectNameError),
    #[error("project catalogue returned an invalid Git remote")]
    GitRemote(#[from] RemoteUrlError),
    #[error("project catalogue returned an invalid repository root")]
    RepositoryRoot(#[from] RepositoryRootError),
}

pub trait ProjectClient: Clone + Send + Sync + 'static {
    fn list_projects(
        &self,
    ) -> impl Future<Output = Result<Vec<ProjectRepository>, ProjectClientError>> + Send;
}
