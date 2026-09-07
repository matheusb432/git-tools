use gtl_application::{
    live_views::{
        LiveViewRecord, recipe_for_record,
        save_live_view::{
            self, LiveViewRejection, SaveLiveView, SaveLiveViewError, SaveLiveViewOk,
            SaveLiveViewOutcome,
        },
    },
    projects::select_unpushed_repositories,
    recipes::{Recipe, RecipeBatch, RecipeBatchKind},
};
use gtl_models::{live_views::LiveSource, recipes::RecipeBatchId};
use gtl_wire::v1::{self, live_view_service_server::LiveViewService};
use tonic::{Request, Response, Status};

use super::{application_notes, project_client_error, run_blocking, unexpected};
use crate::{state::AppState, viewer_process, viewer_runtime};

pub(crate) struct LiveViewGrpcService {
    state: AppState,
}

impl LiveViewGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl LiveViewService for LiveViewGrpcService {
    async fn save_and_present_live_view(
        &self,
        request: Request<v1::SaveAndPresentLiveViewRequest>,
    ) -> Result<Response<v1::SaveAndPresentLiveViewResponse>, Status> {
        let request = request.into_inner();
        let open_viewer = request.open_viewer;
        let request = SaveLiveView {
            comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
            path: super::absolute_path(request.path, "path")?,
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

        let recipe = recipe_for_save(&result);
        let result = save_response(result);
        let presentation = present_saved_live_views(&self.state, open_viewer, recipe)?;

        Ok(Response::new(v1::SaveAndPresentLiveViewResponse {
            result: Some(result),
            presentation,
        }))
    }

    async fn save_and_present_project_live_views(
        &self,
        request: Request<v1::SaveAndPresentProjectLiveViewsRequest>,
    ) -> Result<Response<v1::SaveAndPresentProjectLiveViewsResponse>, Status> {
        let open_viewer = request.into_inner().open_viewer;
        let repos = self
            .state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_client_error(&error))?;
        let state = self.state.clone();
        let results = run_blocking(move || {
            let selected = select_unpushed_repositories::execute(repos, &state.git)
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
                            comparison: gtl_models::live_views::LiveComparison::UnpushedCommits,
                            path: repository.path.as_ref().to_path_buf(),
                        },
                        &state.git,
                        &mut connection,
                        &state.clock,
                    )
                    .map_err(|error| unexpected(error, "save project live view"))
                })
                .collect::<Result<Vec<_>, Status>>()
        })
        .await??;

        let recipes = results
            .iter()
            .filter_map(recipe_for_save)
            .collect::<Vec<_>>();
        let results = results.into_iter().map(save_response).collect();
        let presentation = present_saved_live_views(&self.state, open_viewer, recipes)?;

        Ok(Response::new(v1::SaveAndPresentProjectLiveViewsResponse {
            results,
            presentation,
        }))
    }
}

fn recipe_for_save(result: &SaveLiveViewOk) -> Option<Recipe> {
    let record = match &result.outcome {
        SaveLiveViewOutcome::Created { record } | SaveLiveViewOutcome::Refreshed { record } => {
            record
        }
        SaveLiveViewOutcome::Rejected { .. } => return None,
    };
    Some(recipe_for_record(record))
}

fn present_saved_live_views(
    state: &AppState,
    open_viewer: bool,
    recipes: impl IntoIterator<Item = Recipe>,
) -> Result<Option<v1::DiffPresentation>, Status> {
    let recipes = recipes.into_iter().collect::<Vec<_>>();
    if !open_viewer || recipes.is_empty() {
        return Ok(None);
    }
    if let Err(error) = viewer_process::open() {
        tracing::warn!(error = ?error, "desktop viewer could not be opened for saved live views");
        return Ok(Some(v1::DiffPresentation {
            notes: vec![v1::Note {
                level: v1::NoteLevel::Warning as i32,
                text: format!(
                    "diff live: viewer unavailable ({error}); the live view is saved and will open when the viewer is available"
                ),
            }],
            outcome: Some(v1::diff_presentation::Outcome::ViewerUnavailable(
                v1::ViewerUnavailable {},
            )),
        }));
    }
    viewer_runtime::open_recipe_batch(
        state,
        RecipeBatch {
            batch_id: RecipeBatchId::generate(),
            kind: RecipeBatchKind::Live,
            recipes,
        },
    )
    .map_err(|error| unexpected(error, "open saved live views"))?;
    Ok(Some(v1::DiffPresentation {
        notes: Vec::new(),
        outcome: Some(v1::diff_presentation::Outcome::ViewerOpened(
            v1::ViewerOpened {},
        )),
    }))
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
