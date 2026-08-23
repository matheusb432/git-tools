use gtl_application::{
    live_views::{
        LiveViewRecord,
        save_live_view::{
            self, LiveViewRejection, SaveLiveView, SaveLiveViewError, SaveLiveViewOk,
            SaveLiveViewOutcome,
        },
    },
    projects::select_unpushed_repositories::{self, SelectUnpushedRepositories},
};
use gtl_models::live_views::LiveSource;
use gtl_wire::v1::{self, live_view_service_server::LiveViewService};
use tonic::{Request, Response, Status};

use super::{application_notes, project_client_error, run_blocking, unexpected};
use crate::state::AppState;

#[derive(Clone)]
pub(crate) struct LiveViewApi {
    state: AppState,
}

impl LiveViewApi {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl LiveViewService for LiveViewApi {
    async fn save_live_view(
        &self,
        request: Request<v1::SaveLiveViewRequest>,
    ) -> Result<Response<v1::SaveLiveViewResponse>, Status> {
        let request = SaveLiveView {
            path: super::absolute_path(request.into_inner().path, "path")?,
        };
        let state = self.state.clone();
        let result = run_blocking(move || {
            let mut connection = state
                .database
                .connection_lock()
                .map_err(SaveLiveViewError::from)?;
            save_live_view::execute(request, &state.git, &mut connection, &state.clock)
        })
        .await?
        .map_err(|error| unexpected(error, "save live view"))?;

        Ok(Response::new(save_response(result)))
    }

    async fn save_project_live_views(
        &self,
        _request: Request<v1::SaveProjectLiveViewsRequest>,
    ) -> Result<Response<v1::SaveProjectLiveViewsResponse>, Status> {
        let repos = self
            .state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_client_error(&error))?;
        let state = self.state.clone();
        let results = run_blocking(move || {
            let selected = select_unpushed_repositories::execute(
                SelectUnpushedRepositories { repos },
                &state.git,
            )
            .map_err(|error| unexpected(error, "select project live views"))?;
            let mut connection = state
                .database
                .connection_lock()
                .map_err(SaveLiveViewError::from)
                .map_err(|error| unexpected(error, "open live-view database"))?;
            selected
                .into_iter()
                .map(|repository| {
                    save_live_view::execute(
                        SaveLiveView {
                            path: repository.path.as_ref().to_path_buf(),
                        },
                        &state.git,
                        &mut connection,
                        &state.clock,
                    )
                    .map(save_response)
                    .map_err(|error| unexpected(error, "save project live view"))
                })
                .collect::<Result<Vec<_>, Status>>()
        })
        .await??;

        Ok(Response::new(v1::SaveProjectLiveViewsResponse { results }))
    }
}

fn save_response(result: SaveLiveViewOk) -> v1::SaveLiveViewResponse {
    let notes = application_notes(&result.notes);
    let outcome = match result.outcome {
        SaveLiveViewOutcome::Created { record } => v1::save_live_view_response::Outcome::Saved(
            saved_live_view(record, v1::SaveLiveViewDisposition::Created),
        ),
        SaveLiveViewOutcome::Refreshed { record } => v1::save_live_view_response::Outcome::Saved(
            saved_live_view(record, v1::SaveLiveViewDisposition::Refreshed),
        ),
        SaveLiveViewOutcome::Rejected { rejection } => {
            v1::save_live_view_response::Outcome::Rejected(save_rejection(&rejection))
        }
    };
    v1::SaveLiveViewResponse {
        notes,
        outcome: Some(outcome),
    }
}

fn saved_live_view(
    record: LiveViewRecord,
    disposition: v1::SaveLiveViewDisposition,
) -> v1::SavedLiveView {
    let LiveSource::LocalRepo { path } = record.source;
    v1::SavedLiveView {
        repository_root: path.to_string(),
        display_name: record.display_name.to_string(),
        disposition: disposition as i32,
    }
}

fn save_rejection(rejection: &LiveViewRejection) -> v1::SaveLiveViewRejection {
    let code = match rejection {
        LiveViewRejection::DirNotFound { .. } => v1::SaveLiveViewRejectionCode::DirectoryNotFound,
        LiveViewRejection::DirNotGitRepo { .. } => v1::SaveLiveViewRejectionCode::NotAGitRepository,
    };
    v1::SaveLiveViewRejection {
        code: code as i32,
        detail: rejection.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_projection_keeps_a_machine_code_and_the_service_detail() {
        let rejection = save_rejection(&LiveViewRejection::DirNotGitRepo {
            path: "/tmp/not-a-repo".into(),
        });

        assert_eq!(
            rejection.code(),
            v1::SaveLiveViewRejectionCode::NotAGitRepository
        );
        assert_eq!(
            rejection.detail,
            "The directory `/tmp/not-a-repo` is not a git repository."
        );
    }
}
