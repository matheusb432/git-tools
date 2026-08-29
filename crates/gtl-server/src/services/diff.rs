use gtl_application::{
    diffs::{
        DiffTarget, DiffTargetRequest, RepoRef,
        compute_merge_diff::ComputeMergeDiffError,
        render_diff::{self, RenderDiff, RenderDiffError, RenderDiffOk, RenderDiffOutcome},
        render_diff_subrepos::{
            self, RenderDiffSubrepos, RenderDiffSubreposError, RenderDiffSubreposOk,
            RenderDiffSubreposOutcome,
        },
        render_merge_diff::{self, RenderMergeDiff, RenderMergeDiffError},
    },
    projects::{
        build_recipes::BuildProjectRecipes,
        render_project_diff::{
            self, RenderProjectDiff, RenderProjectDiffError, RenderProjectDiffOk,
        },
        select_unpushed_repositories,
    },
    recipes::{
        Recipe, RecipeBatch, RecipeBatchKind, RecipeOp, RecipeTarget,
        build_recipe::{self, BuildRecipe},
    },
    repositories::{
        build_recipes::BuildRepositoryRecipes,
        find_repository_roots::{self, FindRepositoryRoots},
    },
};
use gtl_models::{
    git::GitRevision, paths::ProjectName, recipes::RecipeBatchId,
    repository::traversal::RepositoryTraversalScope,
};
use gtl_wire::v1::{self, diff_service_server::DiffService};
use tonic::{Request, Response, Status};

use super::{
    application_notes, artifact, project_client_error, repository_root, required, run_blocking,
    unexpected, user_settings_load_error,
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
                .map_err(|_| Status::invalid_argument("name must not be empty"))?,
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
        let recipes = run_blocking(move || build_recipes::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "build subrepositories diff recipes"))?;
        let presentation = match present_snapshot(&self.state, recipes)? {
            SnapshotPresentation::Ready(presentation) => presentation,
            SnapshotPresentation::ViewerUnavailable(error) => {
                let response = self
                    .render_subrepository_diffs(Request::new(render_request))
                    .await?
                    .into_inner();
                presentation_from_render_subrepositories(response, &error)
            }
        };

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
        let repos = self
            .state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_client_error(&error))?;
        let request = BuildProjectRecipes {
            repos,
            operation: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
        };
        let state = self.state.clone();
        let recipes = run_blocking(move || build_recipes::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "build project diff recipes"))?;
        let presentation = match present_snapshot(&self.state, recipes)? {
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
            )
        })
        .await?
        .map_err(render_error)?;

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
            )
        })
        .await?
        .map_err(render_merge_error)?;

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
            )
            .map_err(render_subrepositories_error)
        })
        .await??;

        Ok(Response::new(render_subrepositories_response(result)))
    }

    async fn render_project_repository_diffs(
        &self,
        request: Request<v1::RenderProjectRepositoryDiffsRequest>,
    ) -> Result<Response<v1::RenderProjectRepositoryDiffsResponse>, Status> {
        let root = repository_root(request.into_inner().root, "root")?;
        let repos = self
            .state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_client_error(&error))?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repos = select_unpushed_repositories::execute(repos, &state.git)
                .map_err(|error| unexpected(error, "select project diff repositories"))?;
            if repos.is_empty() {
                return Ok(None);
            }
            let request = RenderProjectDiff {
                root,
                repos: repos
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
            )
            .map(Some)
            .map_err(render_project_error)
        })
        .await??;

        Ok(Response::new(
            result.map_or_else(empty_projects_response, |result| {
                render_project_response(&result)
            }),
        ))
    }
}

fn to_render_request(request: v1::RenderDiffRequest) -> Result<RenderDiff, Status> {
    Ok(RenderDiff {
        cwd: super::absolute_path(request.working_directory, "working_directory")?,
        target: diff_target(request.target)?,
        name: request.name,
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
        return Err(Status::not_found(format!(
            "no git repos found under {}",
            root.as_ref().display()
        )));
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
    DiffTarget::try_from(request.clone())
        .map_err(|error| Status::invalid_argument(error.to_string()))
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
        .map_err(|_| Status::invalid_argument(format!("{field} must not be empty")))
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
    if let Err(error) = viewer_process::open() {
        tracing::warn!(error = ?error, "desktop viewer could not be opened");
        return Ok(SnapshotPresentation::ViewerUnavailable(error));
    }
    viewer_runtime::open_recipe_batch(
        state,
        RecipeBatch {
            batch_id: RecipeBatchId::generate(),
            kind: RecipeBatchKind::Snapshot,
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

fn render_error(error: RenderDiffError) -> Status {
    match error {
        RenderDiffError::InvalidTarget(error) => Status::invalid_argument(error.to_string()),
        RenderDiffError::Settings(error) => user_settings_load_error(error),
        RenderDiffError::Unexpected(error) => unexpected(error, "render diff"),
    }
}

fn render_subrepositories_error(error: RenderDiffSubreposError) -> Status {
    match error {
        RenderDiffSubreposError::InvalidTarget(error) => {
            Status::invalid_argument(error.to_string())
        }
        RenderDiffSubreposError::Settings(error) => user_settings_load_error(error),
        RenderDiffSubreposError::Unexpected(error) => {
            unexpected(error, "render subrepositories diff")
        }
    }
}

fn render_merge_error(error: RenderMergeDiffError) -> Status {
    match error {
        RenderMergeDiffError::Compute(ComputeMergeDiffError::Settings(error)) => {
            user_settings_load_error(error)
        }
        error => unexpected(error, "render merge diff"),
    }
}

fn render_project_error(error: RenderProjectDiffError) -> Status {
    match error {
        RenderProjectDiffError::Settings(error) => user_settings_load_error(error),
        error @ RenderProjectDiffError::Unexpected(_) => unexpected(error, "render project diff"),
    }
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
        outcome: Some(
            v1::render_project_repository_diffs_response::Outcome::Rendered(artifact(
                &result.placement,
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_missing_diff_target_selection() {
        let error = diff_target(Some(v1::DiffTarget { selection: None })).unwrap_err();

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(error.message(), "target.selection is required");
    }

    #[test]
    fn fallback_note_explains_that_the_artifact_was_rendered() {
        let note = viewer_fallback_note(&viewer_process::OpenViewerError::NotInstalled);

        assert_eq!(note.level(), v1::NoteLevel::Warning);
        assert!(note.text.contains("viewer unavailable"));
        assert!(note.text.contains("rendered an artifact"));
    }
}
