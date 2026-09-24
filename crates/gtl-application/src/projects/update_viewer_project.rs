use gtl_models::{
    failure::{ErrorMeta, Failure, Resource},
    projects::ProjectRepository,
};
use gtl_wire::viewer::projects::UpdateViewerProject;
use rusqlite::Connection;

use super::update_project_comparison::{
    self, UpdateProjectComparison, UpdateProjectComparisonError,
};

#[derive(Debug, thiserror::Error, ErrorMeta)]
pub enum UpdateViewerProjectError {
    #[error("project is no longer available")]
    #[meta(failure = Failure::Gone { resource: Resource::Project })]
    NotFound,
    #[error(transparent)]
    #[meta(transparent)]
    Comparison(#[from] UpdateProjectComparisonError),
}

#[cqrsy::command]
pub fn execute(
    request: UpdateViewerProject,
    repositories: &[ProjectRepository],
    connection: &Connection,
) -> Result<(), UpdateViewerProjectError> {
    let project = repositories
        .iter()
        .find(|project| project.path == request.path)
        .ok_or(UpdateViewerProjectError::NotFound)?;
    update_project_comparison::execute(
        UpdateProjectComparison {
            project_name: project.name.clone(),
            comparison_branch: request.comparison_branch,
            expected_comparison_branch: request.expected_comparison_branch,
        },
        connection,
    )?;
    Ok(())
}
