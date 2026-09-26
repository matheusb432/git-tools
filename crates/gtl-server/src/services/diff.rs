use gtl_application::{
    diffs::{
        DiffTarget, DiffTargetRequest, RepoRef,
        render_diff::{self, RenderDiff, RenderDiffOk, RenderDiffOutcome},
        render_diff_subrepos::{
            self, RenderDiffSubrepos, RenderDiffSubreposOk, RenderDiffSubreposOutcome,
        },
        render_merge_diff::{self, RenderMergeDiff},
    },
    projects::{
        build_recipes::BuildProjectRecipes,
        render_project_diff::{self, RenderProjectDiff, RenderProjectDiffOk},
        select_comparison_repositories,
    },
    recipes::{
        Recipe, RecipeBatch, RecipeOp, RecipeTarget,
        build_recipe::{self, BuildRecipe},
    },
    repositories::{
        build_recipes::BuildRepositoryRecipes,
        find_repository_roots::{self, FindRepositoryRoots},
    },
    settings::get_user_settings::{self, GetUserSettings},
};
use gtl_models::{
    failure::RepositoryFailure, git::GitRevision, paths::ProjectName, recipes::RecipeBatchId,
    repository::traversal::RepositoryTraversalScope,
};
use gtl_wire::v1::{self, diff_service_server::DiffService};
use tonic::{Request, Response, Status};

use super::{
    application_notes, artifact, repository_root, required, run_blocking,
    status::{GrpcResultExt as _, invalid_request, status},
    unexpected,
};
use crate::{state::AppState, viewer_process, viewer_runtime};

pub(crate) struct DiffGrpcService {
    state: AppState,
}

impl DiffGrpcService {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl DiffService for DiffGrpcService {
    async fn present_diff(
        &self,
        request: Request<v1::PresentDiffRequest>,
    ) -> Result<Response<v1::PresentDiffResponse>, Status> {
        let request = request.into_inner();
        let render_request = v1::RenderDiffRequest {
            working_directory: request.working_directory.clone(),
            target: request.target.clone(),
            name: request.name.clone(),
        };
        let operation = RecipeOp::Diff {
            target: viewer_recipe_target(request.target)?,
        };
        let request = BuildRecipe {
            repo_path: super::absolute_path(request.working_directory, "working_directory")?,
            operation,
            name: request
                .name
                .map(ProjectName::try_new)
                .transpose()
                .map_err(|_| invalid_request("name"))?,
        };
        let state = self.state.clone();
        let recipe = run_blocking(move || build_recipe::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "build diff recipe"))?;
        let presentation = match present_snapshot(&self.state, vec![recipe])? {
            SnapshotPresentation::Ready(presentation) => presentation,
            SnapshotPresentation::ViewerUnavailable(error) => {
                let response = self
                    .render_diff(Request::new(render_request))
                    .await?
                    .into_inner();
                presentation_from_render_diff(response, &error)
            }
        };

        Ok(Response::new(v1::PresentDiffResponse {
            presentation: Some(presentation),
        }))
    }

    async fn present_merge_diff(
        &self,
        request: Request<v1::PresentMergeDiffRequest>,
    ) -> Result<Response<v1::PresentMergeDiffResponse>, Status> {
        let request = request.into_inner();
        let render_request = v1::RenderMergeDiffRequest {
            working_directory: request.working_directory.clone(),
            base_revision: request.base_revision.clone(),
        };
        let base = optional_revision(request.base_revision, "base_revision")?;
        let request = BuildRecipe {
            repo_path: super::absolute_path(request.working_directory, "working_directory")?,
            operation: RecipeOp::MergeDiff { base, pinned: None },
            name: None,
        };
        let state = self.state.clone();
        let recipe = run_blocking(move || build_recipe::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "build merge diff recipe"))?;
        let presentation = match present_snapshot(&self.state, vec![recipe])? {
            SnapshotPresentation::Ready(presentation) => presentation,
            SnapshotPresentation::ViewerUnavailable(error) => {
                let response = self
                    .render_merge_diff(Request::new(render_request))
                    .await?
                    .into_inner();
                presentation_from_render_merge(response, &error)
            }
        };

        Ok(Response::new(v1::PresentMergeDiffResponse {
            presentation: Some(presentation),
        }))
    }

    async fn present_subrepository_diffs(
        &self,
        request: Request<v1::PresentSubrepositoryDiffsRequest>,
    ) -> Result<Response<v1::PresentSubrepositoryDiffsResponse>, Status> {
        use gtl_application::repositories::build_recipes;

        let request = request.into_inner();
        let render_request = v1::RenderSubrepositoryDiffsRequest {
            root: request.root.clone(),
            target: request.target.clone(),
            include_linked_worktrees: request.include_linked_worktrees,
        };
        let operation = RecipeOp::Diff {
            target: viewer_recipe_target(request.target)?,
        };
        let request = BuildRepositoryRecipes {
            root: super::absolute_path(request.root, "root")?,
            operation,
            scope: traversal_scope(request.include_linked_worktrees),
        };
        let state = self.state.clone();
        let recipes =
            run_blocking(move || build_recipes::execute(request, &state.git, &state.database))
                .await?
                .map_err(|error| unexpected(error, "build subrepositories diff recipes"))?;
        let mut presentation = match present_snapshot(&self.state, recipes.recipes)? {
            SnapshotPresentation::Ready(presentation) => presentation,
            SnapshotPresentation::ViewerUnavailable(error) => {
                let response = self
                    .render_subrepository_diffs(Request::new(render_request))
                    .await?
                    .into_inner();
                presentation_from_render_subrepositories(response, &error)
            }
        };

        presentation.notes.extend(application_notes(&recipes.notes));
        Ok(Response::new(v1::PresentSubrepositoryDiffsResponse {
            presentation: Some(presentation),
        }))
    }

    async fn present_project_repository_diffs(
        &self,
        request: Request<v1::PresentProjectRepositoryDiffsRequest>,
    ) -> Result<Response<v1::PresentProjectRepositoryDiffsResponse>, Status> {
        use gtl_application::projects::build_recipes;

        let root = request.into_inner().root;
        let repos = self.state.projects.list_projects().await.into_grpc()?;
        let request = BuildProjectRecipes {
            repos,
            operation: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
        };
        let state = self.state.clone();
        let recipes =
            run_blocking(move || build_recipes::execute(request, &state.git, &state.database))
                .await?
                .map_err(|error| unexpected(error, "build project diff recipes"))?;
        let mut presentation = match present_snapshot(&self.state, recipes.recipes)? {
            SnapshotPresentation::Ready(presentation) => presentation,
            SnapshotPresentation::ViewerUnavailable(error) => {
                let response = self
                    .render_project_repository_diffs(Request::new(
                        v1::RenderProjectRepositoryDiffsRequest { root },
                    ))
                    .await?
                    .into_inner();
                presentation_from_render_project(response, &error)
            }
        };

        presentation.notes.extend(application_notes(&recipes.notes));
        Ok(Response::new(v1::PresentProjectRepositoryDiffsResponse {
            presentation: Some(presentation),
        }))
    }

    async fn render_diff(
        &self,
        request: Request<v1::RenderDiffRequest>,
    ) -> Result<Response<v1::RenderDiffResponse>, Status> {
        let request = to_render_request(request.into_inner())?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            render_diff::execute(
                request,
                &state.user_settings,
                &state.git,
                &state.artifacts,
                &state.renderer,
                &state.clock,
                &state.database,
            )
        })
        .await?
        .into_grpc()?;

        viewer_runtime::record_project_renders(&self.state, &result.rendered_repositories);
        Ok(Response::new(render_response(result)))
    }

    async fn render_merge_diff(
        &self,
        request: Request<v1::RenderMergeDiffRequest>,
    ) -> Result<Response<v1::RenderMergeDiffResponse>, Status> {
        let request = to_render_merge_request(request.into_inner())?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            render_merge_diff::execute(
                request,
                &state.user_settings,
                &state.git,
                &state.artifacts,
                &state.renderer,
                &state.clock,
                &state.database,
            )
        })
        .await?
        .into_grpc()?;

        viewer_runtime::record_project_renders(&self.state, &result.rendered_repositories);
        Ok(Response::new(render_merge_response(
            &result.placement,
            &result.notes,
        )))
    }

    async fn render_subrepository_diffs(
        &self,
        request: Request<v1::RenderSubrepositoryDiffsRequest>,
    ) -> Result<Response<v1::RenderSubrepositoryDiffsResponse>, Status> {
        let request = request.into_inner();
        let state = self.state.clone();
        let result = run_blocking(move || {
            let request = to_render_subrepositories_request(request, &state.git)?;
            render_diff_subrepos::execute(
                request,
                &state.user_settings,
                &state.git,
                &state.artifacts,
                &state.renderer,
                &state.clock,
                &state.database,
            )
            .into_grpc()
        })
        .await??;

        viewer_runtime::record_project_renders(&self.state, &result.rendered_repositories);
        Ok(Response::new(render_subrepositories_response(result)))
    }

    async fn render_project_repository_diffs(
        &self,
        request: Request<v1::RenderProjectRepositoryDiffsRequest>,
    ) -> Result<Response<v1::RenderProjectRepositoryDiffsResponse>, Status> {
        let root = repository_root(request.into_inner().root, "root")?;
        let repos = self.state.projects.list_projects().await.into_grpc()?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repos = select_comparison_repositories::execute(repos, &state.git, &state.database)
                .map_err(|error| unexpected(error, "select project diff repositories"))?;
            if repos.repositories.is_empty() {
                return Ok((None, repos.notes));
            }
            let request = RenderProjectDiff {
                root,
                repos: repos
                    .repositories
                    .into_iter()
                    .map(|repo| RepoRef {
                        top: repo.path,
                        label: repo.label,
                    })
                    .collect(),
            };
            render_project_diff::execute(
                request,
                &state.user_settings,
                &state.git,
                &state.artifacts,
                &state.renderer,
                &state.clock,
                &state.database,
            )
            .map(|result| (Some(result), repos.notes))
            .into_grpc()
        })
        .await??;

        let (result, notes) = result;
        if let Some(result) = &result {
            viewer_runtime::record_project_renders(&self.state, &result.rendered_repositories);
        }
        Ok(Response::new({
            let mut response = result.map_or_else(empty_projects_response, |result| {
                render_project_response(&result)
            });
            response.notes.extend(application_notes(&notes));
            response
        }))
    }
}

fn to_render_request(request: v1::RenderDiffRequest) -> Result<RenderDiff, Status> {
    Ok(RenderDiff {
        cwd: super::absolute_path(request.working_directory, "working_directory")?,
        target: diff_target(request.target)?,
        name: request
            .name
            .map(ProjectName::try_new)
            .transpose()
            .map_err(|_| invalid_request("name"))?,
    })
}

fn to_render_merge_request(request: v1::RenderMergeDiffRequest) -> Result<RenderMergeDiff, Status> {
    Ok(RenderMergeDiff {
        cwd: super::absolute_path(request.working_directory, "working_directory")?,
        base: optional_revision(request.base_revision, "base_revision")?,
    })
}

fn to_render_subrepositories_request(
    request: v1::RenderSubrepositoryDiffsRequest,
    git: &impl gtl_application::ports::GitClient,
) -> Result<RenderDiffSubrepos, Status> {
    let root = repository_root(request.root, "root")?;
    let repositories = find_repository_roots::execute(
        FindRepositoryRoots {
            root: root.as_ref().to_path_buf(),
            scope: traversal_scope(request.include_linked_worktrees),
        },
        git,
    )
    .map_err(|error| unexpected(error, "discover subrepositories for diff"))?;
    if repositories.is_empty() {
        return Err(status(&RepositoryFailure::NoRepositories {
            root: root.as_ref().to_path_buf(),
        }));
    }
    Ok(RenderDiffSubrepos {
        root,
        target: diff_target(request.target)?,
        repos: repositories
            .into_iter()
            .map(|repository| RepoRef {
                top: repository.path,
                label: repository.label,
            })
            .collect(),
    })
}

fn diff_target(target: Option<v1::DiffTarget>) -> Result<DiffTargetRequest, Status> {
    let target = validated_diff_target(target)?;
    Ok(DiffTargetRequest::from(&target))
}

fn validated_diff_target(target: Option<v1::DiffTarget>) -> Result<DiffTarget, Status> {
    let selection = required(required(target, "target")?.selection, "target.selection")?;
    let request = match selection {
        v1::diff_target::Selection::Unpushed(_) => DiffTargetRequest::Unpushed,
        v1::diff_target::Selection::BaseRevision(rev) => DiffTargetRequest::Base { rev },
        v1::diff_target::Selection::RevisionRange(range) => DiffTargetRequest::Range { range },
        v1::diff_target::Selection::MergeBase(base) => DiffTargetRequest::Merge { base },
        v1::diff_target::Selection::LastCommitCount(count) => DiffTargetRequest::Last { count },
    };
    DiffTarget::try_from(request.clone()).into_grpc()
}

fn viewer_recipe_target(target: Option<v1::DiffTarget>) -> Result<RecipeTarget, Status> {
    Ok(match validated_diff_target(target)? {
        DiffTarget::Unpushed { pinned } => RecipeTarget::Unpushed { pinned },
        DiffTarget::Base(rev) => RecipeTarget::Base { rev },
        DiffTarget::Range { range, pinned } => RecipeTarget::Range { range, pinned },
        DiffTarget::Merge { base, pinned } => RecipeTarget::Merge { base, pinned },
        DiffTarget::Last { count, pinned } => RecipeTarget::Last { count, pinned },
    })
}

fn optional_revision(
    revision: Option<String>,
    field: &'static str,
) -> Result<Option<GitRevision>, Status> {
    revision
        .map(GitRevision::try_new)
        .transpose()
        .map_err(|_| invalid_request(field))
}

const fn traversal_scope(include_linked_worktrees: bool) -> RepositoryTraversalScope {
    if include_linked_worktrees {
        RepositoryTraversalScope::IncludeLinkedWorktrees
    } else {
        RepositoryTraversalScope::ExcludeLinkedWorktrees
    }
}

enum SnapshotPresentation {
    Ready(v1::DiffPresentation),
    ViewerUnavailable(viewer_process::OpenViewerError),
}

fn present_snapshot(
    state: &AppState,
    recipes: Vec<Recipe>,
) -> Result<SnapshotPresentation, Status> {
    if recipes.is_empty() {
        return Ok(SnapshotPresentation::Ready(v1::DiffPresentation {
            notes: Vec::new(),
            outcome: Some(v1::diff_presentation::Outcome::Empty(v1::Empty {})),
        }));
    }
    let settings = get_user_settings::execute(GetUserSettings, &state.user_settings).into_grpc()?;
    if let Err(error) = viewer_process::open(settings.focus_window_on_diff()) {
        tracing::warn!(error = ?error, "desktop viewer could not be opened");
        return Ok(SnapshotPresentation::ViewerUnavailable(error));
    }
    viewer_runtime::open_recipe_batch(
        state,
        RecipeBatch {
            batch_id: RecipeBatchId::generate(),
            recipes,
        },
    )
    .map_err(|error| unexpected(error, "open viewer recipe batch"))?;
    Ok(SnapshotPresentation::Ready(v1::DiffPresentation {
        notes: Vec::new(),
        outcome: Some(v1::diff_presentation::Outcome::ViewerOpened(
            v1::ViewerOpened {},
        )),
    }))
}

fn viewer_fallback_note(error: &viewer_process::OpenViewerError) -> v1::Note {
    v1::Note {
        level: v1::NoteLevel::Warning as i32,
        text: format!("diff: viewer unavailable ({error}); rendered an artifact instead"),
    }
}

fn presentation_from_render_diff(
    response: v1::RenderDiffResponse,
    error: &viewer_process::OpenViewerError,
) -> v1::DiffPresentation {
    v1::DiffPresentation {
        notes: prepend_fallback_note(response.notes, error),
        outcome: response.outcome.map(|outcome| match outcome {
            v1::render_diff_response::Outcome::Rendered(artifact) => {
                v1::diff_presentation::Outcome::Rendered(artifact)
            }
            v1::render_diff_response::Outcome::Empty(empty) => {
                v1::diff_presentation::Outcome::Empty(empty)
            }
        }),
    }
}

fn presentation_from_render_merge(
    response: v1::RenderMergeDiffResponse,
    error: &viewer_process::OpenViewerError,
) -> v1::DiffPresentation {
    v1::DiffPresentation {
        notes: prepend_fallback_note(response.notes, error),
        outcome: response.outcome.map(|outcome| match outcome {
            v1::render_merge_diff_response::Outcome::Rendered(artifact) => {
                v1::diff_presentation::Outcome::Rendered(artifact)
            }
            v1::render_merge_diff_response::Outcome::Empty(empty) => {
                v1::diff_presentation::Outcome::Empty(empty)
            }
        }),
    }
}

fn presentation_from_render_subrepositories(
    response: v1::RenderSubrepositoryDiffsResponse,
    error: &viewer_process::OpenViewerError,
) -> v1::DiffPresentation {
    v1::DiffPresentation {
        notes: prepend_fallback_note(response.notes, error),
        outcome: response.outcome.map(|outcome| match outcome {
            v1::render_subrepository_diffs_response::Outcome::Rendered(artifact) => {
                v1::diff_presentation::Outcome::Rendered(artifact)
            }
            v1::render_subrepository_diffs_response::Outcome::Empty(empty) => {
                v1::diff_presentation::Outcome::Empty(empty)
            }
        }),
    }
}

fn presentation_from_render_project(
    response: v1::RenderProjectRepositoryDiffsResponse,
    error: &viewer_process::OpenViewerError,
) -> v1::DiffPresentation {
    v1::DiffPresentation {
        notes: prepend_fallback_note(response.notes, error),
        outcome: response.outcome.map(|outcome| match outcome {
            v1::render_project_repository_diffs_response::Outcome::Rendered(artifact) => {
                v1::diff_presentation::Outcome::Rendered(artifact)
            }
            v1::render_project_repository_diffs_response::Outcome::Empty(empty) => {
                v1::diff_presentation::Outcome::Empty(empty)
            }
        }),
    }
}

fn prepend_fallback_note(
    notes: Vec<v1::Note>,
    error: &viewer_process::OpenViewerError,
) -> Vec<v1::Note> {
    std::iter::once(viewer_fallback_note(error))
        .chain(notes)
        .collect()
}

fn render_merge_response(
    placement: &gtl_application::ports::PlacedArtifact,
    notes: &[gtl_application::shared::notes::Note],
) -> v1::RenderMergeDiffResponse {
    v1::RenderMergeDiffResponse {
        notes: application_notes(notes),
        outcome: Some(v1::render_merge_diff_response::Outcome::Rendered(artifact(
            placement,
        ))),
    }
}

fn render_response(result: RenderDiffOk) -> v1::RenderDiffResponse {
    match result.outcome {
        RenderDiffOutcome::Rendered(placement) => v1::RenderDiffResponse {
            notes: application_notes(&result.notes),
            outcome: Some(v1::render_diff_response::Outcome::Rendered(artifact(
                &placement,
            ))),
        },
        RenderDiffOutcome::Empty => v1::RenderDiffResponse {
            notes: application_notes(&result.notes),
            outcome: Some(v1::render_diff_response::Outcome::Empty(v1::Empty {})),
        },
    }
}

fn empty_projects_response() -> v1::RenderProjectRepositoryDiffsResponse {
    v1::RenderProjectRepositoryDiffsResponse {
        notes: Vec::new(),
        outcome: Some(v1::render_project_repository_diffs_response::Outcome::Empty(v1::Empty {})),
    }
}

fn render_subrepositories_response(
    result: RenderDiffSubreposOk,
) -> v1::RenderSubrepositoryDiffsResponse {
    match result.outcome {
        RenderDiffSubreposOutcome::Rendered(placement) => v1::RenderSubrepositoryDiffsResponse {
            notes: application_notes(&result.notes),
            outcome: Some(v1::render_subrepository_diffs_response::Outcome::Rendered(
                artifact(&placement),
            )),
        },
        RenderDiffSubreposOutcome::Empty => v1::RenderSubrepositoryDiffsResponse {
            notes: application_notes(&result.notes),
            outcome: Some(v1::render_subrepository_diffs_response::Outcome::Empty(
                v1::Empty {},
            )),
        },
    }
}

fn render_project_response(
    result: &RenderProjectDiffOk,
) -> v1::RenderProjectRepositoryDiffsResponse {
    v1::RenderProjectRepositoryDiffsResponse {
        notes: application_notes(&result.notes),
        outcome: Some(match &result.placement {
            Some(placement) => {
                v1::render_project_repository_diffs_response::Outcome::Rendered(artifact(placement))
            }
            None => v1::render_project_repository_diffs_response::Outcome::Empty(v1::Empty {}),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_missing_diff_target_selection() {
        let error = diff_target(Some(v1::DiffTarget { selection: None })).unwrap_err();

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(
            super::super::status::decoded_failure(&error),
            Some(gtl_models::failure::Failure::InvalidRequest {
                field: "target.selection".into()
            })
        );
    }

    #[test]
    fn fallback_note_explains_that_the_artifact_was_rendered() {
        let note = viewer_fallback_note(&viewer_process::OpenViewerError::NotInstalled);

        assert_eq!(note.level(), v1::NoteLevel::Warning);
        assert!(note.text.contains("viewer unavailable"));
        assert!(note.text.contains("rendered an artifact"));
    }
}
