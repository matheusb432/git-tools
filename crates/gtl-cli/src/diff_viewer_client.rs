use std::{num::NonZeroU32, path::PathBuf};

use anyhow::Context as _;
use gtl_models::{
    diffs::{CommitId, PinnedRange},
    git::{GitRange, GitRevision},
    paths::{AbsoluteFilePath, ProjectName, RepositoryRoot},
    recipes::RecipeBatchId,
};
use gtl_wire::{
    recipes::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget},
    v1,
};

use crate::commands::diff::DiffOutcome;

pub(crate) struct PreparedRecipeBatch {
    batch: Option<v1::RecipeBatch>,
}

pub(crate) struct RenderedDiffResult {
    notes: Vec<v1::Note>,
    outcome: Option<RenderedDiffOutcome>,
}

enum RenderedDiffOutcome {
    Rendered(v1::Artifact),
    Empty,
}

pub(crate) fn forward_prepared(
    response: impl Into<PreparedRecipeBatch>,
) -> anyhow::Result<DiffOutcome> {
    let batch = open_recipes(response)?;
    if batch.recipes.is_empty() {
        return Ok(DiffOutcome::Empty);
    }
    forward(&batch)?;
    Ok(DiffOutcome::Forwarded)
}

pub(crate) fn finish_render(
    response: impl Into<RenderedDiffResult>,
    viewer_error: Option<&anyhow::Error>,
) -> anyhow::Result<DiffOutcome> {
    let response = response.into();
    if let Some(error) = viewer_error {
        eprintln!(
            "diff: viewer unavailable ({}); rendering an artifact instead",
            crate::error_text(error)
        );
    }
    print_notes(&response.notes)?;
    match response
        .outcome
        .context("gtl-server returned no diff outcome")?
    {
        RenderedDiffOutcome::Rendered(artifact) => {
            let path = AbsoluteFilePath::try_new(PathBuf::from(artifact.path))
                .context("gtl-server returned a non-absolute artifact path")?;
            match v1::ArtifactPlacement::try_from(artifact.placement) {
                Ok(v1::ArtifactPlacement::Created | v1::ArtifactPlacement::Reused) => {}
                Ok(v1::ArtifactPlacement::Unspecified) | Err(_) => {
                    anyhow::bail!("gtl-server returned an invalid artifact placement")
                }
            }
            println!("{}", crate::commands::file_url(path.as_path()));
            Ok(DiffOutcome::Rendered(path))
        }
        RenderedDiffOutcome::Empty => Ok(DiffOutcome::Empty),
    }
}

fn forward(batch: &OpenRecipes) -> anyhow::Result<()> {
    let bin = crate::viewer::resolve_viewer_bin()
        .context("gtl-viewer is not installed; cannot forward the recipe batch")?;
    let token = gtl_wire::recipes::encode_token(batch)
        .context("failed to encode the viewer recipe batch")?;
    crate::detached_process::spawn(&bin, &[token.as_str()])
        .context("failed to spawn gtl-viewer to forward the recipe batch")
}

fn open_recipes(response: impl Into<PreparedRecipeBatch>) -> anyhow::Result<OpenRecipes> {
    let response = response.into();
    let batch = response
        .batch
        .context("gtl-server returned no prepared recipe batch")?;
    Ok(OpenRecipes {
        batch_id: batch
            .batch_id
            .parse::<RecipeBatchId>()
            .context("gtl-server returned an invalid recipe batch ID")?,
        kind: match v1::RecipeBatchKind::try_from(batch.kind) {
            Ok(v1::RecipeBatchKind::Snapshot) => RecipeBatchKind::Snapshot,
            Ok(v1::RecipeBatchKind::Live) => RecipeBatchKind::Live,
            Ok(v1::RecipeBatchKind::Unspecified) | Err(_) => {
                anyhow::bail!("gtl-server returned an invalid recipe batch kind")
            }
        },
        recipes: batch
            .recipes
            .into_iter()
            .map(open_recipe)
            .collect::<anyhow::Result<Vec<_>>>()?,
    })
}

impl From<v1::PrepareDiffResponse> for PreparedRecipeBatch {
    fn from(response: v1::PrepareDiffResponse) -> Self {
        Self {
            batch: response.batch,
        }
    }
}

impl From<v1::PrepareMergeDiffResponse> for PreparedRecipeBatch {
    fn from(response: v1::PrepareMergeDiffResponse) -> Self {
        Self {
            batch: response.batch,
        }
    }
}

impl From<v1::PrepareSubrepositoryDiffsResponse> for PreparedRecipeBatch {
    fn from(response: v1::PrepareSubrepositoryDiffsResponse) -> Self {
        Self {
            batch: response.batch,
        }
    }
}

impl From<v1::PrepareProjectRepositoryDiffsResponse> for PreparedRecipeBatch {
    fn from(response: v1::PrepareProjectRepositoryDiffsResponse) -> Self {
        Self {
            batch: response.batch,
        }
    }
}

impl From<v1::RenderDiffResponse> for RenderedDiffResult {
    fn from(response: v1::RenderDiffResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_diff_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_diff_response::Outcome::Empty(_) => RenderedDiffOutcome::Empty,
            }),
        }
    }
}

impl From<v1::RenderMergeDiffResponse> for RenderedDiffResult {
    fn from(response: v1::RenderMergeDiffResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_merge_diff_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_merge_diff_response::Outcome::Empty(_) => RenderedDiffOutcome::Empty,
            }),
        }
    }
}

impl From<v1::RenderSubrepositoryDiffsResponse> for RenderedDiffResult {
    fn from(response: v1::RenderSubrepositoryDiffsResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_subrepository_diffs_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_subrepository_diffs_response::Outcome::Empty(_) => {
                    RenderedDiffOutcome::Empty
                }
            }),
        }
    }
}

impl From<v1::RenderProjectRepositoryDiffsResponse> for RenderedDiffResult {
    fn from(response: v1::RenderProjectRepositoryDiffsResponse) -> Self {
        Self {
            notes: response.notes,
            outcome: response.outcome.map(|outcome| match outcome {
                v1::render_project_repository_diffs_response::Outcome::Rendered(artifact) => {
                    RenderedDiffOutcome::Rendered(artifact)
                }
                v1::render_project_repository_diffs_response::Outcome::Empty(_) => {
                    RenderedDiffOutcome::Empty
                }
            }),
        }
    }
}

fn open_recipe(recipe: v1::Recipe) -> anyhow::Result<Recipe> {
    let repository_root = RepositoryRoot::try_new(PathBuf::from(recipe.repository_root))
        .context("gtl-server returned a non-absolute recipe repository root")?;
    let op = match recipe
        .operation
        .context("gtl-server returned a recipe without an operation")?
    {
        v1::recipe::Operation::Diff(diff) => RecipeOp::Diff {
            target: recipe_target(
                diff.target
                    .context("gtl-server returned a diff recipe without a target")?,
            )?,
        },
        v1::recipe::Operation::MergeDiff(merge) => RecipeOp::MergeDiff {
            base: merge
                .base_revision
                .map(GitRevision::try_new)
                .transpose()
                .context("gtl-server returned an empty merge recipe base")?,
            pinned: merge.pinned.map(pinned_range).transpose()?,
        },
    };
    Ok(Recipe {
        source: RecipeSource::LocalRepo(repository_root),
        op,
        name: recipe
            .name
            .map(ProjectName::try_new)
            .transpose()
            .context("gtl-server returned an empty recipe name")?,
    })
}

fn recipe_target(target: v1::RecipeTarget) -> anyhow::Result<RecipeTarget> {
    Ok(
        match target
            .selection
            .context("gtl-server returned a recipe target without a selection")?
        {
            v1::recipe_target::Selection::Unpushed(target) => RecipeTarget::Unpushed {
                pinned: target.pinned.map(pinned_range).transpose()?,
            },
            v1::recipe_target::Selection::Base(target) => RecipeTarget::Base {
                rev: GitRevision::try_new(target.revision)
                    .context("gtl-server returned an empty recipe revision")?,
            },
            v1::recipe_target::Selection::Range(target) => RecipeTarget::Range {
                range: GitRange::try_new(target.range)
                    .context("gtl-server returned an empty recipe range")?,
                pinned: target.pinned.map(pinned_range).transpose()?,
            },
            v1::recipe_target::Selection::Merge(target) => RecipeTarget::Merge {
                base: GitRevision::try_new(target.base_revision)
                    .context("gtl-server returned an empty merge recipe base")?,
                pinned: target.pinned.map(pinned_range).transpose()?,
            },
            v1::recipe_target::Selection::Last(target) => RecipeTarget::Last {
                count: NonZeroU32::new(target.commit_count)
                    .context("gtl-server returned a zero recipe commit count")?,
                pinned: target.pinned.map(pinned_range).transpose()?,
            },
        },
    )
}

fn pinned_range(range: v1::PinnedRange) -> anyhow::Result<PinnedRange> {
    Ok(PinnedRange {
        base: CommitId::try_from(range.base_commit_id)
            .context("gtl-server returned an invalid pinned base commit")?,
        head: CommitId::try_from(range.head_commit_id)
            .context("gtl-server returned an invalid pinned head commit")?,
    })
}

fn print_notes(notes: &[v1::Note]) -> anyhow::Result<()> {
    for note in notes {
        match v1::NoteLevel::try_from(note.level) {
            Ok(v1::NoteLevel::Info) => println!("{}", note.text),
            Ok(v1::NoteLevel::Warning) => eprintln!("{}", note.text),
            Ok(v1::NoteLevel::Error) => anyhow::bail!(note.text.clone()),
            Ok(v1::NoteLevel::Unspecified) | Err(_) => {
                anyhow::bail!("gtl-server returned an invalid diff note level")
            }
        }
    }
    Ok(())
}
