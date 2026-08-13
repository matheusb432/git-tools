//! Projection from sample_project's shared project catalogue into GTL-managed repositories.

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
use gtl_models::managed::ManagedRepo;

#[derive(Clone, Debug)]
pub struct ManagedRepoClient {
    client: SampleCliClient,
    home: Option<PathBuf>,
}

impl ManagedRepoClient {
    /// Uses the `sample_project` executable from `PATH` and the platform home directory.
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            client: SampleCliClient::default(),
            home: BaseDirs::new().map(|directories| directories.home_dir().to_path_buf()),
        }
    }

    /// Lists active sample_project projects as GTL-managed repositories.
    ///
    /// # Errors
    /// Returns an error when sample_project is unavailable, its project data is invalid, or the local home
    /// directory cannot be resolved.
    pub async fn list_projects(&self) -> Result<Vec<ManagedRepo>, ProjectClientError> {
        let projects = self
            .client
            .list_projects()
            .await
            .map_err(project_client_error_from_shared)?;
        let home = self
            .home
            .as_deref()
            .ok_or_else(home_directory_unavailable)?;

        Ok(projects
            .into_iter()
            .map(|project| project_to_managed_repo(project, home))
            .collect())
    }
}

impl ProjectClient for ManagedRepoClient {
    async fn list_projects(&self) -> Result<Vec<ManagedRepo>, ProjectClientError> {
        ManagedRepoClient::list_projects(self).await
    }
}

fn project_to_managed_repo(project: Project, home: &Path) -> ManagedRepo {
    ManagedRepo {
        name: project.title.to_string(),
        path: project.source.resolve_directory(home),
        remote: project
            .git_remote
            .map_or_else(String::new, |remote| remote.to_string()),
    }
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
    use gtl_models::managed::ManagedRepo;

    use super::{project_client_error_from_shared, project_to_managed_repo};

    #[test]
    fn projects_shared_project_metadata_into_a_managed_repo() {
        let project = project(Some("git@example.test:tools/git-tools.git"));

        let repo = project_to_managed_repo(project, Path::new("/home/u"));

        assert_eq!(
            repo,
            ManagedRepo {
                name: "git-tools".into(),
                path: "/home/u/tools/git-tools".into(),
                remote: "git@example.test:tools/git-tools.git".into(),
            }
        );
    }

    #[test]
    fn projects_a_missing_git_remote_as_an_empty_value() {
        let project = project(None);

        let repo = project_to_managed_repo(project, Path::new("/home/u"));

        assert_eq!(repo.remote, "");
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
