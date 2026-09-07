use gtl_application::projects::catalogue::{ProjectCatalogueError, create_project::CreateProject};
use gtl_models::projects::catalogue::{
    Project, ProjectAffiliation, ProjectGroups, ProjectMetadata, ProjectMutation,
    ProjectMutationOutcome, ProjectOperationMode, ProjectStatus,
};
use gtl_wire::v1;
use tonic::Status;

pub(super) fn create_request(request: v1::CreateProjectRequest) -> Result<CreateProject, Status> {
    let input = super::super::required(request.project, "project")?;
    let source = super::super::required(input.source, "project.source")?;
    let source = match super::super::required(source.source, "project.source.directory")? {
        v1::project_source::Source::Directory(directory) => {
            directory.path.try_into().map_err(invalid)?
        }
    };
    let affiliation = match v1::ProjectAffiliation::try_from(input.affiliation) {
        Ok(v1::ProjectAffiliation::Personal) => ProjectAffiliation::Personal,
        Ok(v1::ProjectAffiliation::Work) => ProjectAffiliation::Work,
        _ => return Err(Status::invalid_argument("project affiliation is required")),
    };
    let groups = input
        .groups
        .into_iter()
        .map(TryInto::try_into)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    Ok(CreateProject {
        id: request.project_id.try_into().map_err(invalid)?,
        metadata: ProjectMetadata {
            title: input.title.try_into().map_err(invalid)?,
            source,
            git_remote: input
                .git_remote
                .map(TryInto::try_into)
                .transpose()
                .map_err(invalid)?,
            mux_session_name: input.mux_session_name.try_into().map_err(invalid)?,
            affiliation,
            color: input
                .color
                .map(TryInto::try_into)
                .transpose()
                .map_err(invalid)?,
            groups: ProjectGroups::try_new(groups).map_err(invalid)?,
        },
        include_in_full_export: input.include_in_full_export.unwrap_or(true),
    })
}

pub(super) fn mode(raw: i32) -> Result<ProjectOperationMode, Status> {
    match v1::ProjectOperationMode::try_from(raw) {
        Ok(v1::ProjectOperationMode::Preview) => Ok(ProjectOperationMode::Preview),
        Ok(v1::ProjectOperationMode::Apply) => Ok(ProjectOperationMode::Apply),
        _ => Err(Status::invalid_argument(
            "project operation mode is required",
        )),
    }
}

pub(super) fn invalid(error: impl std::fmt::Display) -> Status {
    Status::invalid_argument(error.to_string())
}

pub(super) fn error(error: ProjectCatalogueError) -> Status {
    match error {
        ProjectCatalogueError::NotFound => Status::not_found("project was not found"),
        ProjectCatalogueError::AlreadyExists => {
            Status::already_exists("project ID, title, source, or session name already exists")
        }
        ProjectCatalogueError::LimitExceeded => {
            Status::resource_exhausted("project catalogue exceeds its limit")
        }
        ProjectCatalogueError::InvalidData(source) => {
            tracing::error!(error = ?source, "project catalogue contains invalid data");
            Status::data_loss("project catalogue contains invalid data")
        }
        ProjectCatalogueError::Database(source) => {
            tracing::error!(error = ?source, "project database operation failed");
            Status::internal("project database operation failed")
        }
    }
}

pub(super) fn project(project: Project) -> v1::Project {
    v1::Project {
        id: project.id.to_string(),
        title: project.metadata.title.to_string(),
        source: Some(v1::ProjectSource {
            source: Some(v1::project_source::Source::Directory(v1::DirectorySource {
                path: project.metadata.source.to_string(),
            })),
        }),
        git_remote: project.metadata.git_remote.map(|remote| remote.to_string()),
        mux_session_name: project.metadata.mux_session_name.to_string(),
        status: match project.status {
            ProjectStatus::Active => v1::ProjectStatus::Active,
            ProjectStatus::Paused => v1::ProjectStatus::Paused,
        } as i32,
        affiliation: match project.metadata.affiliation {
            ProjectAffiliation::Personal => v1::ProjectAffiliation::Personal,
            ProjectAffiliation::Work => v1::ProjectAffiliation::Work,
        } as i32,
        color: project.metadata.color.map(|color| color.to_string()),
        groups: project
            .metadata
            .groups
            .as_slice()
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

pub(super) fn get_response(value: Project) -> v1::GetProjectResponse {
    let project = project(value);
    v1::GetProjectResponse {
        id: project.id,
        title: project.title,
        source: project.source,
        git_remote: project.git_remote,
        mux_session_name: project.mux_session_name,
        status: project.status,
        affiliation: project.affiliation,
        color: project.color,
        groups: project.groups,
    }
}

pub(super) fn outcome(outcome: ProjectMutationOutcome) -> i32 {
    (match outcome {
        ProjectMutationOutcome::Changed => v1::ProjectMutationOutcome::Changed,
        ProjectMutationOutcome::Unchanged => v1::ProjectMutationOutcome::Unchanged,
    }) as i32
}

pub(super) fn mutations(values: Vec<ProjectMutation>) -> Vec<v1::ProjectMutation> {
    values
        .into_iter()
        .map(|mutation| v1::ProjectMutation {
            project_id: mutation.id.to_string(),
            outcome: outcome(mutation.outcome),
        })
        .collect()
}

pub(super) fn lock_error(error: &anyhow::Error) -> Status {
    tracing::warn!(error = ?error, "project database is unavailable");
    Status::unavailable("project database is unavailable")
}
