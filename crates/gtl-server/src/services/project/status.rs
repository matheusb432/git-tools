use gtl_application::repositories::get_repository_statuses;
use gtl_models::repository::traversal::RepositoryTarget;
use gtl_wire::v1;
use tonic::{Response, Status};

use super::super::{project_client_error, task_join};
use crate::{services::repository::status::statuses_response, state::AppState};

pub(super) async fn get(
    state: AppState,
) -> Result<Response<v1::RepositoryStatusesResponse>, Status> {
    let repos = state
        .projects
        .list_projects()
        .await
        .map_err(|error| project_client_error(&error))?
        .into_iter()
        .map(|repo| RepositoryTarget {
            label: repo.name,
            path: repo.path,
        })
        .collect();
    let results = tokio::task::spawn_blocking(move || {
        get_repository_statuses::execute(
            get_repository_statuses::GetRepositoryStatuses { repos },
            &state.git,
        )
    })
    .await
    .map_err(|error| task_join(&error))?;

    Ok(Response::new(statuses_response(&results)))
}
