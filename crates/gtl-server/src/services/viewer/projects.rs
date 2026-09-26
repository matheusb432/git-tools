use gtl_application::projects::{get_viewer_project_status, list_viewer_projects};
use gtl_models::failure::{ErrorClass, Failure, ProjectFailure, Resource};
use gtl_wire::{proto, v1};
use tonic::{Request, Response, Status};

use super::super::{
    run_blocking,
    status::{GrpcResultExt as _, invalid_request, private, status},
    unexpected,
};
use crate::{state::AppState, viewer_runtime};

pub(super) async fn list_viewer_projects(
    state: &AppState,
    request: Request<v1::ListViewerProjectsRequest>,
) -> Result<Response<v1::ListViewerProjectsResponse>, Status> {
    let request = proto::viewer::projects::decode_list(request.into_inner())
        .map_err(|_| invalid_request("page"))?;
    if matches!(
        request.sort,
        Some(
            gtl_models::settings::ProjectsSort::Changes
                | gtl_models::settings::ProjectsSort::ChangesAscending
                | gtl_models::settings::ProjectsSort::Branch
                | gtl_models::settings::ProjectsSort::BranchDescending
        )
    ) {
        refresh_status_index(state).await?;
    }
    let state = state.clone();
    let page = run_blocking(move || {
        let connection = state.database.connection_lock()?;
        list_viewer_projects::execute(&request, &connection)
    })
    .await?
    .map_err(|error| project_catalogue_error(&error))?;
    Ok(Response::new(proto::viewer::projects::encode_page(&page)))
}

async fn refresh_status_index(state: &AppState) -> Result<(), Status> {
    use std::sync::{Arc, atomic::AtomicBool};
    let cancellation = Arc::new(AtomicBool::new(false));
    let _cancel = CancelStatusIndex(cancellation.clone());
    let state = state.clone();
    tokio::time::timeout(std::time::Duration::from_secs(25), async move {
        let request = state
            .viewer_project_index_requests
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| private(ErrorClass::Unavailable, "project sorting stopped"))?;
        let permit = state
            .viewer_project_status_workers
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| private(ErrorClass::Unavailable, "project status workers stopped"))?;
        run_blocking(move || {
            let _request = request;
            let _permit = permit;
            let _scope = gtl_infra::git_client::status_context::StatusContextScope::new(
                cancellation.clone(),
            );
            refresh_status_index_blocking(&state, &cancellation)
        })
        .await?
        .map_err(|error| project_catalogue_error(&error))
    })
    .await
    .map_err(|_| private(ErrorClass::DeadlineExceeded, "project sorting timed out"))?
}

fn refresh_status_index_blocking(
    state: &AppState,
    cancellation: &std::sync::atomic::AtomicBool,
) -> anyhow::Result<()> {
    let projects = {
        let connection = state.database.connection_lock()?;
        list_viewer_projects::status_refresh_candidates(&connection)?
    };
    for project in projects {
        anyhow::ensure!(
            !cancellation.load(std::sync::atomic::Ordering::Relaxed),
            "project sorting cancelled"
        );
        let status = get_viewer_project_status::execute(project.clone(), &state.git);
        anyhow::ensure!(
            !cancellation.load(std::sync::atomic::Ordering::Relaxed),
            "project sorting cancelled"
        );
        {
            let connection = state.database.connection_lock()?;
            gtl_application::projects::status_index::record(
                &project,
                status.as_ref().ok(),
                &connection,
            )?;
        }
        if let Ok(status) = status {
            state
                .viewer_project_status_cache
                .lock()
                .map_err(|_| anyhow::anyhow!("status cache poisoned"))?
                .insert(project.path, status, std::time::Instant::now());
        }
    }
    Ok(())
}

struct CancelStatusIndex(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for CancelStatusIndex {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(super) async fn get_viewer_project_status(
    state: &AppState,
    request: Request<v1::GetViewerProjectStatusRequest>,
) -> Result<Response<v1::GetViewerProjectStatusResponse>, Status> {
    let request = proto::viewer::projects::decode_get_status(request.into_inner())
        .map_err(|_| invalid_request("project_id"))?;
    let admission = state
        .viewer_project_status_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| status(&Failure::Busy))?;
    let state = state.clone();
    let status = tokio::time::timeout(std::time::Duration::from_secs(30), async move {
        let permit = state
            .viewer_project_status_workers
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| private(ErrorClass::Unavailable, "project status workers stopped"))?;
        run_blocking(move || {
            let _admission = admission;
            let _permit = permit;
            let project = {
                let connection = state
                    .database
                    .connection_lock()
                    .map_err(|error| unexpected(error, "open project database"))?;
                list_viewer_projects::get_project(&request.project_id, &connection)
                    .map_err(|error| unexpected(error, "get viewer project"))?
                    .ok_or_else(project_gone)?
            };
            let status = get_viewer_project_status::execute(project.clone(), &state.git)
                .map_err(|error| unexpected(error, "get viewer project status"))?;
            let connection = state
                .database
                .connection_lock()
                .map_err(|error| unexpected(error, "open project database"))?;
            gtl_application::projects::status_index::record(&project, Some(&status), &connection)
                .map_err(|error| unexpected(error, "index project status"))?;
            Ok::<_, Status>(status)
        })
        .await?
    })
    .await
    .map_err(|_| private(ErrorClass::DeadlineExceeded, "project status timed out"))??;
    Ok(Response::new(
        proto::viewer::projects::encode_project_status(status),
    ))
}

pub(super) async fn open_viewer_project(
    state: &AppState,
    request: Request<v1::OpenViewerProjectRequest>,
) -> Result<Response<v1::OpenViewerProjectResponse>, Status> {
    use gtl_application::projects::open_viewer_project::{self, OpenProjectComparison};
    let project = proto::viewer::projects::decode_open(request.into_inner())
        .map_err(|_| invalid_request("comparison"))?;
    let repositories = state
        .projects
        .list_projects()
        .await
        .map_err(|error| project_catalogue_error(&error))?;
    let state = state.clone();
    let runtime_state = state.clone();
    let work = run_blocking(move || {
        open_viewer_project::execute(
            OpenProjectComparison {
                project,
                repositories,
            },
            &state.git,
            &state.viewer,
        )
    })
    .await?
    .into_grpc()?;
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
        .map_err(|error| invalid_request(error.field().unwrap_or("project")))?;
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
        update_viewer_project::execute(request, &repositories, &connection).into_grpc()
    })
    .await??;
    Ok(Response::new(v1::UpdateViewerProjectResponse {}))
}

pub(super) fn project_catalogue_error(error: &impl std::fmt::Debug) -> Status {
    tracing::warn!(error = ?error, "viewer project catalogue is unavailable");
    status(&ProjectFailure::CatalogueUnavailable)
}

fn project_gone() -> Status {
    status(&Failure::Gone {
        resource: Resource::Project,
    })
}
