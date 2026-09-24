use gtl_application::projects::catalogue::create_project::CreateProject;
use gtl_models::{
    failure::ErrorClass,
    projects::catalogue::{
        Project, ProjectGroups, ProjectMetadata, ProjectMutation, ProjectMutationOutcome,
        ProjectOperationMode, ProjectStatus,
    },
};
use gtl_wire::v1;
use tonic::Status;

use super::super::{
    required,
    status::{invalid_request, private_error},
};

pub(super) fn create_request(request: v1::CreateProjectRequest) -> Result<CreateProject, Status> {
    let input = required(request.project, "project")?;
    let source = required(input.source, "project.source")?;
    let source = match required(source.source, "project.source.directory")? {
        v1::project_source::Source::Directory(directory) => directory
            .path
            .try_into()
            .map_err(|_| invalid_request("project.source.directory.path"))?,
    };
    let groups = input
        .groups
        .into_iter()
        .map(TryInto::try_into)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid_request("project.groups"))?;
    Ok(CreateProject {
        id: request
            .project_id
            .try_into()
            .map_err(|_| invalid_request("project_id"))?,
        metadata: ProjectMetadata {
            title: input
                .title
                .try_into()
                .map_err(|_| invalid_request("project.title"))?,
            source,
            git_remote: input
                .git_remote
                .map(TryInto::try_into)
                .transpose()
                .map_err(|_| invalid_request("project.git_remote"))?,
            color: input
                .color
                .map(TryInto::try_into)
                .transpose()
                .map_err(|_| invalid_request("project.color"))?,
            groups: ProjectGroups::try_new(groups)
                .map_err(|_| invalid_request("project.groups"))?,
        },
        include_in_full_export: input.include_in_full_export.unwrap_or(true),
    })
}

pub(super) fn mode(raw: i32) -> Result<ProjectOperationMode, Status> {
    match v1::ProjectOperationMode::try_from(raw) {
        Ok(v1::ProjectOperationMode::Preview) => Ok(ProjectOperationMode::Preview),
        Ok(v1::ProjectOperationMode::Apply) => Ok(ProjectOperationMode::Apply),
        _ => Err(invalid_request("mode")),
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
        status: match project.status {
            ProjectStatus::Active => v1::ProjectStatus::Active,
            ProjectStatus::Paused => v1::ProjectStatus::Paused,
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
        status: project.status,
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
    private_error(ErrorClass::Unavailable, error)
}
