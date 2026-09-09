use gtl_application::projects::list_viewer_projects;
use gtl_wire::{proto, v1};
use tonic::{Request, Response, Status};

use super::super::{run_blocking, unexpected};
use crate::{state::AppState, viewer_runtime};

pub(super) async fn list_viewer_projects(
    state: &AppState,
    _request: Request<v1::ListViewerProjectsRequest>,
) -> Result<Response<v1::ListViewerProjectsResponse>, Status> {
    let repositories = state
        .projects
        .list_projects()
        .await
        .map_err(|error| project_catalogue_error(&error))?;
    let state = state.clone();
    let projects = run_blocking(move || {
        let connection = state.database.connection_lock()?;
        list_viewer_projects::execute(repositories, &state.git, &connection)
    })
    .await?
    .map_err(|error| unexpected(error, "list viewer projects"))?;
    Ok(Response::new(v1::ListViewerProjectsResponse {
        projects: projects
            .into_iter()
            .map(proto::viewer::projects::encode_project)
            .collect(),
    }))
}

pub(super) async fn open_viewer_project(
    state: &AppState,
    request: Request<v1::OpenViewerProjectRequest>,
) -> Result<Response<v1::OpenViewerProjectResponse>, Status> {
    use gtl_application::projects::open_viewer_project::{
        self, OpenProjectComparison, OpenViewerProjectError,
    };
    let project = proto::viewer::projects::decode_open(request.into_inner())
        .map_err(|_| Status::invalid_argument("invalid project comparison"))?;
    let repositories = state
        .projects
        .list_projects()
        .await
        .map_err(|error| project_catalogue_error(&error))?;
    let state = state.clone();
    let runtime_state = state.clone();
    let work = run_blocking(move || {
        let mut connection = state
            .database
            .connection_lock()
            .map_err(OpenViewerProjectError::from)?;
        open_viewer_project::execute(
            OpenProjectComparison {
                project,
                repositories,
            },
            &state.git,
            &mut connection,
            &state.clock,
            &state.viewer,
        )
    })
    .await?
    .map_err(|error| match error {
        OpenViewerProjectError::NotFound => Status::not_found("project is no longer available"),
        OpenViewerProjectError::Unexpected(error) => unexpected(error, "open project comparison"),
    })?;
    let tab_id = work.ticket().tab_id.into();
    viewer_runtime::spawn_recipe(runtime_state, work);
    Ok(Response::new(v1::OpenViewerProjectResponse { tab_id }))
}

pub(super) async fn update_viewer_project(
    state: &AppState,
    request: Request<v1::UpdateViewerProjectRequest>,
) -> Result<Response<v1::UpdateViewerProjectResponse>, Status> {
    use gtl_application::projects::update_viewer_project::{self};
    let request = proto::viewer::projects::decode_update(request.into_inner())
        .map_err(|_| Status::invalid_argument("invalid local comparison branch or project path"))?;
    let repositories = state
        .projects
        .list_projects()
        .await
        .map_err(|error| project_catalogue_error(&error))?;
    let state = state.clone();
    run_blocking(move || {
        let connection = state
            .database
            .connection_lock()
            .map_err(|error| unexpected(error, "open project database"))?;
        update_viewer_project::execute(request, &repositories, &connection)
            .map_err(update_project_error)
    })
    .await??;
    Ok(Response::new(v1::UpdateViewerProjectResponse {}))
}

pub(super) fn project_catalogue_error(
    error: &gtl_application::ports::ProjectClientError,
) -> Status {
    tracing::warn!(error = ?error, "viewer project catalogue is unavailable");
    Status::failed_precondition("project catalogue is unavailable")
}

fn update_project_error(
    error: gtl_application::projects::update_viewer_project::UpdateViewerProjectError,
) -> Status {
    use gtl_application::projects::{
        update_project_comparison::UpdateProjectComparisonError,
        update_viewer_project::UpdateViewerProjectError,
    };
    match error {
        UpdateViewerProjectError::NotFound => Status::not_found("project is no longer available"),
        UpdateViewerProjectError::Comparison(UpdateProjectComparisonError::Conflict) => {
            Status::aborted("project comparison changed")
        }
        UpdateViewerProjectError::Comparison(UpdateProjectComparisonError::Database(error)) => {
            unexpected(error, "update project comparison")
        }
    }
}
