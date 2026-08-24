//! Projection from sample_project's shared project catalogue into GTL project repositories.

use std::path::{Path, PathBuf};

use sample_project_client::{
    ProjectClient as _,
    project::{Project, grpc::SampleGrpcClient},
};
use directories::BaseDirs;
use gtl_application::ports::{
    ProjectCatalogueConfigurationError, ProjectCatalogueDataError,
    ProjectCatalogueUnavailableError, ProjectClient, ProjectClientError,
};
use gtl_models::{
    git::RemoteUrl,
    paths::{ProjectName, RepositoryRoot},
    projects::ProjectRepository,
};

#[derive(Clone, Debug)]
pub struct ProjectRepositoryClient {
    home: Option<PathBuf>,
}

impl ProjectRepositoryClient {
    /// Uses sample_project's local authenticated endpoint and the platform home directory.
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            home: BaseDirs::new().map(|directories| directories.home_dir().to_path_buf()),
        }
    }

    /// Lists active sample_project projects as GTL project repositories.
    ///
    /// # Errors
    /// Returns an error when sample_project is misconfigured or unavailable, its project data is invalid, or
    /// the local home directory cannot be resolved.
    pub async fn list_projects(&self) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        let client = SampleGrpcClient::connect_local()
            .await
            .map_err(project_client_error_from_shared)?;
        let projects = client
            .list_projects()
            .await
            .map_err(project_client_error_from_shared)?;
        let home = self
            .home
            .as_deref()
            .ok_or(ProjectCatalogueConfigurationError::HomeDirectoryUnavailable)?;

        let repositories = projects
            .into_iter()
            .map(|project| project_to_repository(project, home))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(repositories)
    }
}

impl ProjectClient for ProjectRepositoryClient {
    async fn list_projects(&self) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        ProjectRepositoryClient::list_projects(self).await
    }
}

fn project_to_repository(
    project: Project,
    home: &Path,
) -> Result<ProjectRepository, ProjectCatalogueDataError> {
    let remote = project
        .git_remote
        .map(|remote| RemoteUrl::try_new(remote.to_string()))
        .transpose()?;
    let name = ProjectName::try_new(project.title.to_string())?;
    let path = RepositoryRoot::try_new(project.source.resolve_directory(home))?;
    Ok(ProjectRepository { name, path, remote })
}

fn project_client_error_from_shared(error: sample_project_client::ProjectClientError) -> ProjectClientError {
    match error {
        sample_project_client::ProjectClientError::InvalidConfiguration(source) => {
            ProjectCatalogueConfigurationError::Dependency(anyhow::Error::new(source)).into()
        }
        sample_project_client::ProjectClientError::Unavailable(source) => {
            ProjectCatalogueUnavailableError::Dependency(anyhow::Error::new(source)).into()
        }
        sample_project_client::ProjectClientError::InvalidData(source) => {
            ProjectCatalogueDataError::Dependency(anyhow::Error::new(source)).into()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io, path::Path};

    use sample_project_client::project::{
        Project, ProjectAffiliation, ProjectDirectorySource, ProjectGitRemote, ProjectGroups,
        ProjectId, ProjectMuxSessionName, ProjectName, ProjectStatus,
    };
    use gtl_application::ports::{
        ProjectCatalogueConfigurationError, ProjectCatalogueDataError,
        ProjectCatalogueUnavailableError, ProjectClientError,
    };
    use gtl_models::{
        git::RemoteUrl,
        paths::{self, RepositoryRoot},
        projects::ProjectRepository,
    };

    use super::{project_client_error_from_shared, project_to_repository};

    #[test]
    fn projects_shared_project_metadata_into_a_project_repository() {
        let project = project(Some("git@example.test:tools/git-tools.git"));

        let repo = project_to_repository(project, Path::new("/home/u")).unwrap();

        assert_eq!(
            repo,
            ProjectRepository {
                name: paths::ProjectName::try_from("git-tools").unwrap(),
                path: RepositoryRoot::try_new("/home/u/tools/git-tools".into()).unwrap(),
                remote: Some(RemoteUrl::try_new("git@example.test:tools/git-tools.git").unwrap(),),
            }
        );
    }

    #[test]
    fn projects_a_missing_git_remote_as_an_empty_value() {
        let project = project(None);

        let repo = project_to_repository(project, Path::new("/home/u")).unwrap();

        assert_eq!(repo.remote, None);
    }

    #[test]
    fn rejects_a_repository_root_that_is_not_absolute() {
        let error = project_to_repository(project(None), Path::new("relative-home")).unwrap_err();

        assert!(matches!(
            error,
            ProjectCatalogueDataError::RepositoryRoot(_)
        ));
    }

    #[test]
    fn classifies_a_missing_shared_dependency_as_invalid_configuration() -> anyhow::Result<()> {
        let shared: sample_project_client::ProjectClientError =
            sample_project_client::ProjectClientConfigurationError::provider(io::Error::new(
                io::ErrorKind::NotFound,
                "fixture executable is missing",
            ))
            .into();
        let error = project_client_error_from_shared(shared);

        let ProjectClientError::InvalidConfiguration(
            ProjectCatalogueConfigurationError::Dependency(source),
        ) = error
        else {
            anyhow::bail!("missing dependency changed error classification");
        };
        assert_eq!(
            source.root_cause().to_string(),
            "fixture executable is missing"
        );
        Ok(())
    }

    #[test]
    fn preserves_transient_shared_project_failures_and_their_source() -> anyhow::Result<()> {
        let shared: sample_project_client::ProjectClientError =
            sample_project_client::ProjectClientUnavailableError::provider(io::Error::new(
                io::ErrorKind::TimedOut,
                "fixture timed out",
            ))
            .into();
        let error = project_client_error_from_shared(shared);

        let ProjectClientError::Unavailable(ProjectCatalogueUnavailableError::Dependency(source)) =
            error
        else {
            anyhow::bail!("transient failure changed error classification");
        };
        assert_eq!(source.root_cause().to_string(), "fixture timed out");
        Ok(())
    }

    #[test]
    fn preserves_invalid_shared_project_data_and_its_source() -> anyhow::Result<()> {
        let shared: sample_project_client::ProjectClientError =
            sample_project_client::ProjectClientDataError::provider(io::Error::new(
                io::ErrorKind::InvalidData,
                "fixture invalid data",
            ))
            .into();
        let error = project_client_error_from_shared(shared);

        let ProjectClientError::InvalidData(ProjectCatalogueDataError::Dependency(source)) = error
        else {
            anyhow::bail!("invalid shared data changed error classification");
        };
        assert_eq!(source.root_cause().to_string(), "fixture invalid data");
        Ok(())
    }

    fn project(git_remote: Option<&str>) -> Project {
        Project {
            id: ProjectId::try_new("GTL").expect("fixture project ID is valid"),
            title: ProjectName::try_new("git-tools").expect("fixture project name is valid"),
            source: ProjectDirectorySource::try_from_home_relative_path("tools/git-tools")
                .expect("fixture project source is valid")
                .into(),
            git_remote: git_remote.map(|remote| {
                ProjectGitRemote::try_new(remote).expect("fixture Git remote is valid")
            }),
            mux_session_name: ProjectMuxSessionName::try_new("git-tools")
                .expect("fixture mux session name is valid"),
            status: ProjectStatus::Active,
            affiliation: ProjectAffiliation::Personal,
            color: None,
            groups: ProjectGroups::default(),
        }
    }
}
