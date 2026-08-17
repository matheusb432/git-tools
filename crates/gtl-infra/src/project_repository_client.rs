//! Projection from sample_project's shared project catalogue into GTL project repositories.

use std::{
    io,
    path::{Path, PathBuf},
};

use sample_project_client::{
    ProjectClient as _,
    project::{Project, cli::SampleCliClient},
};
use directories::BaseDirs;
use gtl_application::ports::{ProjectClient, ProjectClientError};
use gtl_models::{
    git::RemoteUrl,
    paths::{ProjectName, RepositoryRoot},
    projects::ProjectRepository,
};

#[derive(Clone, Debug)]
pub struct ProjectRepositoryClient {
    client: SampleCliClient,
    home: Option<PathBuf>,
}

impl ProjectRepositoryClient {
    /// Uses the `sample_project` executable from `PATH` and the platform home directory.
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            client: SampleCliClient::default(),
            home: BaseDirs::new().map(|directories| directories.home_dir().to_path_buf()),
        }
    }

    /// Lists active sample_project projects as GTL project repositories.
    ///
    /// # Errors
    /// Returns an error when sample_project is unavailable, its project data is invalid, or the local home
    /// directory cannot be resolved.
    pub async fn list_projects(&self) -> Result<Vec<ProjectRepository>, ProjectClientError> {
        let projects = self
            .client
            .list_projects()
            .await
            .map_err(project_client_error_from_shared)?;
        let home = self
            .home
            .as_deref()
            .ok_or_else(home_directory_unavailable)?;

        projects
            .into_iter()
            .map(|project| project_to_repository(project, home))
            .collect()
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
) -> Result<ProjectRepository, ProjectClientError> {
    let remote = project
        .git_remote
        .map(|remote| RemoteUrl::try_new(remote.to_string()))
        .transpose()
        .map_err(|source| ProjectClientError::InvalidData {
            message: format!("project '{}' has an empty Git remote", project.title),
            source: Box::new(source),
        })?;
    let project_title = project.title.to_string();
    let name = ProjectName::try_new(project_title.clone()).map_err(|source| {
        ProjectClientError::InvalidData {
            message: format!("project '{project_title}' has an invalid project name"),
            source: Box::new(source),
        }
    })?;
    let path =
        RepositoryRoot::try_new(project.source.resolve_directory(home)).map_err(|source| {
            ProjectClientError::InvalidData {
                message: format!("project '{project_title}' has a non-absolute source directory"),
                source: Box::new(source),
            }
        })?;
    Ok(ProjectRepository { name, path, remote })
}

fn project_client_error_from_shared(error: sample_project_client::ProjectClientError) -> ProjectClientError {
    match error {
        sample_project_client::ProjectClientError::Unavailable { message, source } => {
            ProjectClientError::Unavailable { message, source }
        }
        sample_project_client::ProjectClientError::InvalidData { message, source } => {
            ProjectClientError::InvalidData { message, source }
        }
    }
}

fn home_directory_unavailable() -> ProjectClientError {
    ProjectClientError::Unavailable {
        message: "resolving the home directory for sample_project project sources".into(),
        source: Box::new(io::Error::new(
            io::ErrorKind::NotFound,
            "home directory is unavailable",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::{io, path::Path};

    use sample_project_client::project::{
        Project, ProjectAffiliation, ProjectDirectorySource, ProjectGitRemote, ProjectGroups,
        ProjectId, ProjectMuxSessionName, ProjectName, ProjectStatus,
    };
    use gtl_application::ports::ProjectClientError;
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
    fn preserves_shared_project_error_classification_and_source() -> anyhow::Result<()> {
        let unavailable =
            project_client_error_from_shared(sample_project_client::ProjectClientError::unavailable(
                "running sample_project",
                io::Error::other("fixture unavailable"),
            ));
        assert!(matches!(
            unavailable,
            ProjectClientError::Unavailable { .. }
        ));

        let invalid =
            project_client_error_from_shared(sample_project_client::ProjectClientError::invalid_data(
                "parsing projects",
                io::Error::new(io::ErrorKind::InvalidData, "fixture invalid data"),
            ));
        let ProjectClientError::InvalidData { message, source } = invalid else {
            anyhow::bail!("invalid shared data changed error classification");
        };
        assert_eq!(message, "parsing projects");
        assert_eq!(source.to_string(), "fixture invalid data");
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
