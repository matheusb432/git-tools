use gtl_application::{
    diffs::{
        DiffTarget, DiffTargetRequest, RepoRef,
        render_diff::{self, RenderDiff, RenderDiffError, RenderDiffOk, RenderDiffOutcome},
        render_diff_subrepos::{
            self, RenderDiffSubrepos, RenderDiffSubreposError, RenderDiffSubreposOk,
            RenderDiffSubreposOutcome,
        },
        render_merge_diff::{self, RenderMergeDiff},
    },
    projects::{
        build_recipes::BuildProjectRecipes,
        render_project_diff::{self, RenderProjectDiff, RenderProjectDiffOk},
        select_unpushed_repositories::{self, SelectUnpushedRepositories},
    },
    recipes::build_recipe::{self, BuildRecipe},
    repositories::{
        build_recipes::BuildRepositoryRecipes,
        find_repository_roots::{self, FindRepositoryRoots},
    },
};
use gtl_models::{
    git::GitRevision, paths::ProjectName, recipes::RecipeBatchId,
    repository::traversal::RepositoryTraversalScope,
};
use gtl_wire::{
    recipes::{PinnedRange, Recipe, RecipeOp, RecipeSource, RecipeTarget},
    v1::{self, diff_service_server::DiffService},
};
use tonic::{Request, Response, Status};

use super::{
    application_notes, artifact, project_client_error, repository_root, required, run_blocking,
    unexpected,
};
use crate::state::AppState;

#[derive(Clone)]
pub(crate) struct DiffApi {
    state: AppState,
}

impl DiffApi {
    pub(crate) const fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl DiffService for DiffApi {
    async fn prepare(
        &self,
        request: Request<v1::PrepareDiffRequest>,
    ) -> Result<Response<v1::PrepareDiffResponse>, Status> {
        let request = request.into_inner();
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
            .map_err(|error| unexpected(error, "prepare diff recipe"))?;

        Ok(Response::new(prepared_response(vec![recipe])))
    }

    async fn prepare_merge(
        &self,
        request: Request<v1::PrepareMergeDiffRequest>,
    ) -> Result<Response<v1::PrepareDiffResponse>, Status> {
        let request = request.into_inner();
        let base = optional_revision(request.base_revision, "base_revision")?;
        let request = BuildRecipe {
            repo_path: super::absolute_path(request.working_directory, "working_directory")?,
            operation: RecipeOp::MergeDiff { base, pinned: None },
            name: None,
        };
        let state = self.state.clone();
        let recipe = run_blocking(move || build_recipe::execute(request, &state.git))
            .await?
            .map_err(|error| unexpected(error, "prepare merge diff recipe"))?;

        Ok(Response::new(prepared_response(vec![recipe])))
    }

    async fn prepare_subrepositories(
        &self,
        request: Request<v1::PrepareSubrepositoriesDiffRequest>,
    ) -> Result<Response<v1::PrepareDiffResponse>, Status> {
        use gtl_application::repositories::build_recipes;

        let request = request.into_inner();
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
            .map_err(|error| unexpected(error, "prepare subrepositories diff recipes"))?;

        Ok(Response::new(prepared_response(recipes)))
    }

    async fn prepare_projects(
        &self,
        _request: Request<v1::PrepareProjectsDiffRequest>,
    ) -> Result<Response<v1::PrepareDiffResponse>, Status> {
        use gtl_application::projects::build_recipes;

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
            .map_err(|error| unexpected(error, "prepare project diff recipes"))?;

        Ok(Response::new(prepared_response(recipes)))
    }

    async fn render(
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

    async fn render_merge(
        &self,
        request: Request<v1::RenderMergeDiffRequest>,
    ) -> Result<Response<v1::RenderDiffResponse>, Status> {
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
        .map_err(|error| unexpected(error, "render merge diff"))?;

        Ok(Response::new(rendered_response(
            &result.placement,
            &result.notes,
        )))
    }

    async fn render_subrepositories(
        &self,
        request: Request<v1::RenderSubrepositoriesDiffRequest>,
    ) -> Result<Response<v1::RenderDiffResponse>, Status> {
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

    async fn render_projects(
        &self,
        request: Request<v1::RenderProjectsDiffRequest>,
    ) -> Result<Response<v1::RenderDiffResponse>, Status> {
        let root = repository_root(request.into_inner().root, "root")?;
        let repos = self
            .state
            .projects
            .list_projects()
            .await
            .map_err(|error| project_client_error(&error))?;
        let state = self.state.clone();
        let result = run_blocking(move || {
            let repos = select_unpushed_repositories::execute(
                SelectUnpushedRepositories { repos },
                &state.git,
            )
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
            .map_err(|error| unexpected(error, "render project diff"))
        })
        .await??;

        Ok(Response::new(
            result.map_or_else(empty_response, |result| render_project_response(&result)),
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
    request: v1::RenderSubrepositoriesDiffRequest,
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

fn prepared_response(recipes: Vec<Recipe>) -> v1::PrepareDiffResponse {
    v1::PrepareDiffResponse {
        batch: Some(v1::RecipeBatch {
            batch_id: RecipeBatchId::generate().to_string(),
            kind: v1::RecipeBatchKind::Snapshot as i32,
            recipes: recipes.into_iter().map(recipe).collect(),
        }),
    }
}

fn recipe(recipe: Recipe) -> v1::Recipe {
    let RecipeSource::LocalRepo(repository_root) = recipe.source;
    let operation = match recipe.op {
        RecipeOp::Diff { target } => v1::recipe::Operation::Diff(v1::DiffRecipe {
            target: Some(recipe_target(target)),
        }),
        RecipeOp::MergeDiff { base, pinned } => {
            v1::recipe::Operation::MergeDiff(v1::MergeDiffRecipe {
                base_revision: base.map(|revision| revision.to_string()),
                pinned: pinned.as_ref().map(pinned_range),
            })
        }
    };
    v1::Recipe {
        repository_root: repository_root.to_string(),
        operation: Some(operation),
        name: recipe.name.map(|name| name.to_string()),
    }
}

fn recipe_target(target: RecipeTarget) -> v1::RecipeTarget {
    let selection = match target {
        RecipeTarget::Unpushed { pinned } => {
            v1::recipe_target::Selection::Unpushed(v1::UnpushedRecipeTarget {
                pinned: pinned.as_ref().map(pinned_range),
            })
        }
        RecipeTarget::Base { rev } => v1::recipe_target::Selection::Base(v1::BaseRecipeTarget {
            revision: rev.to_string(),
        }),
        RecipeTarget::Range { range, pinned } => {
            v1::recipe_target::Selection::Range(v1::RangeRecipeTarget {
                range: range.to_string(),
                pinned: pinned.as_ref().map(pinned_range),
            })
        }
        RecipeTarget::Merge { base, pinned } => {
            v1::recipe_target::Selection::Merge(v1::MergeRecipeTarget {
                base_revision: base.to_string(),
                pinned: pinned.as_ref().map(pinned_range),
            })
        }
        RecipeTarget::Last { count, pinned } => {
            v1::recipe_target::Selection::Last(v1::LastRecipeTarget {
                commit_count: count.get(),
                pinned: pinned.as_ref().map(pinned_range),
            })
        }
    };
    v1::RecipeTarget {
        selection: Some(selection),
    }
}

fn pinned_range(range: &PinnedRange) -> v1::PinnedRange {
    v1::PinnedRange {
        base_commit_id: range.base.to_string(),
        head_commit_id: range.head.to_string(),
    }
}

fn render_error(error: RenderDiffError) -> Status {
    match error {
        RenderDiffError::InvalidTarget(error) => Status::invalid_argument(error.to_string()),
        RenderDiffError::Settings(error) => unexpected(error, "load diff settings"),
        RenderDiffError::Unexpected(error) => unexpected(error, "render diff"),
    }
}

fn render_subrepositories_error(error: RenderDiffSubreposError) -> Status {
    match error {
        RenderDiffSubreposError::InvalidTarget(error) => {
            Status::invalid_argument(error.to_string())
        }
        RenderDiffSubreposError::Settings(error) => unexpected(error, "load diff settings"),
        RenderDiffSubreposError::Unexpected(error) => {
            unexpected(error, "render subrepositories diff")
        }
    }
}

fn rendered_response(
    placement: &gtl_application::ports::PlacedArtifact,
    notes: &[gtl_application::shared::notes::Note],
) -> v1::RenderDiffResponse {
    v1::RenderDiffResponse {
        notes: application_notes(notes),
        outcome: Some(v1::render_diff_response::Outcome::Rendered(artifact(
            placement,
        ))),
    }
}

fn render_response(result: RenderDiffOk) -> v1::RenderDiffResponse {
    match result.outcome {
        RenderDiffOutcome::Rendered(placement) => rendered_response(&placement, &result.notes),
        RenderDiffOutcome::Empty => v1::RenderDiffResponse {
            notes: application_notes(&result.notes),
            outcome: Some(v1::render_diff_response::Outcome::Empty(v1::Empty {})),
        },
    }
}

fn empty_response() -> v1::RenderDiffResponse {
    v1::RenderDiffResponse {
        notes: Vec::new(),
        outcome: Some(v1::render_diff_response::Outcome::Empty(v1::Empty {})),
    }
}

fn render_subrepositories_response(result: RenderDiffSubreposOk) -> v1::RenderDiffResponse {
    match result.outcome {
        RenderDiffSubreposOutcome::Rendered(placement) => {
            rendered_response(&placement, &result.notes)
        }
        RenderDiffSubreposOutcome::Empty => v1::RenderDiffResponse {
            notes: application_notes(&result.notes),
            outcome: Some(v1::render_diff_response::Outcome::Empty(v1::Empty {})),
        },
    }
}

fn render_project_response(result: &RenderProjectDiffOk) -> v1::RenderDiffResponse {
    rendered_response(&result.placement, &result.notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_missing_diff_target_selection() {
        let error = diff_target(Some(v1::DiffTarget { selection: None }))
            .expect_err("missing selection must fail");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(error.message(), "target.selection is required");
    }

    #[test]
    fn prepared_batch_uses_typed_recipe_fields() {
        let response = prepared_response(vec![Recipe {
            source: RecipeSource::LocalRepo(repository_root("/repo".into(), "fixture").unwrap()),
            op: RecipeOp::Diff {
                target: RecipeTarget::Unpushed { pinned: None },
            },
            name: Some(ProjectName::try_new("repo").unwrap()),
        }]);

        let batch = response.batch.expect("prepared response has a batch");
        assert_eq!(batch.kind(), v1::RecipeBatchKind::Snapshot);
        assert_eq!(batch.recipes[0].repository_root, "/repo");
        assert!(matches!(
            batch.recipes[0].operation,
            Some(v1::recipe::Operation::Diff(_))
        ));
    }
}
