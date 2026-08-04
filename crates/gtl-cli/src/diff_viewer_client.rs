use std::path::PathBuf;

use anyhow::Context as _;
use gtl_application::{
    diffs::{DiffTarget, PinnedRange},
    ports::{
        DiffRenderOutcome, DiffRenderRequest, DiffRenderResponse, DiffViewerBatch,
        DiffViewerClient, DiffViewerRecipe, DiffViewerRecipeOperation,
    },
    shared::notes::{Note, NoteLevel},
};
use gtl_contracts::{
    envelope::{Envelope, Outcome},
    recipes::{OpenRecipes, Recipe, RecipeBatchKind, RecipeOp, RecipeSource, RecipeTarget},
};

use crate::client::HttpClient;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CliDiffViewerClient;

impl DiffViewerClient for CliDiffViewerClient {
    fn forward(&self, batch: &DiffViewerBatch) -> anyhow::Result<()> {
        let recipes = batch.recipes.iter().map(to_wire_recipe).collect();
        let wire = OpenRecipes {
            batch_id: batch.batch_id.clone(),
            kind: RecipeBatchKind::Snapshot,
            recipes,
        };
        let bin = crate::viewer::resolve_viewer_bin()
            .context("gtl-viewer is not installed; cannot forward the recipe batch")?;
        let token = gtl_contracts::recipes::encode_token(&wire)
            .context("failed to encode the viewer recipe batch")?;
        gtl_infra::detached_process::spawn(&bin, &[token.as_str()])
            .context("failed to spawn gtl-viewer to forward the recipe batch")
    }

    fn render(&self, request: &DiffRenderRequest) -> anyhow::Result<DiffRenderResponse> {
        let client = HttpClient::ensure_daemon()?;
        let envelope = match request {
            DiffRenderRequest::Diff(request) => client.render_diff(request)?,
            DiffRenderRequest::MergeDiff(request) => client.render_merge_diff(request)?,
            DiffRenderRequest::SquashPreview(request) => client.render_squash_preview(request)?,
            DiffRenderRequest::Subrepos(request) => client.render_diff_subrepos(request)?,
            DiffRenderRequest::ManagedAll(request) => client.render_diff_all(request)?,
        };
        from_wire_response(envelope)
    }
}

fn from_wire_response(
    envelope: Envelope<gtl_contracts::diffs::RenderDiffData>,
) -> anyhow::Result<DiffRenderResponse> {
    let notes = envelope
        .notes
        .iter()
        .filter_map(|note| match note.level {
            gtl_contracts::envelope::NoteLevel::Info => Some(Note::info(note.text.clone())),
            gtl_contracts::envelope::NoteLevel::Warn => Some(Note::warn(note.text.clone())),
            gtl_contracts::envelope::NoteLevel::Error => None,
        })
        .collect();
    let outcome = match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            DiffRenderOutcome::Rendered(PathBuf::from(data.artifact))
        }
        Outcome::Empty => DiffRenderOutcome::Empty,
        Outcome::Error => anyhow::bail!("{}", crate::commands::error_text(&envelope.notes)),
    };
    Ok(DiffRenderResponse { outcome, notes })
}

fn to_wire_recipe(recipe: &DiffViewerRecipe) -> Recipe {
    Recipe {
        source: RecipeSource::LocalRepo(recipe.source.clone()),
        op: match &recipe.operation {
            DiffViewerRecipeOperation::Diff(target) => RecipeOp::Diff {
                target: to_wire_target(target),
            },
            DiffViewerRecipeOperation::MergeDiff { base, pinned } => RecipeOp::MergeDiff {
                base: base.clone(),
                pinned: pinned.as_ref().map(to_wire_pin),
            },
            DiffViewerRecipeOperation::SquashPreview { pinned } => RecipeOp::SquashPreview {
                pinned: pinned.as_ref().map(to_wire_pin),
            },
        },
        name: recipe.name.clone(),
    }
}

fn to_wire_target(target: &DiffTarget) -> RecipeTarget {
    match target {
        DiffTarget::Unpushed { pinned } => RecipeTarget::Unpushed {
            pinned: pinned.as_ref().map(to_wire_pin),
        },
        DiffTarget::Base(rev) => RecipeTarget::Base { rev: rev.clone() },
        DiffTarget::Range { range, pinned } => RecipeTarget::Range {
            range: range.clone(),
            pinned: pinned.as_ref().map(to_wire_pin),
        },
        DiffTarget::Merge { base, pinned } => RecipeTarget::Merge {
            base: base.clone(),
            pinned: pinned.as_ref().map(to_wire_pin),
        },
        DiffTarget::Last { count, pinned } => RecipeTarget::Last {
            count: *count,
            pinned: pinned.as_ref().map(to_wire_pin),
        },
    }
}

fn to_wire_pin(pin: &PinnedRange) -> gtl_contracts::recipes::PinnedRange {
    gtl_contracts::recipes::PinnedRange {
        base: pin.base.clone(),
        head: pin.head.clone(),
    }
}

pub(crate) fn print_notes(notes: &[Note]) {
    for note in notes {
        match note.level {
            NoteLevel::Info => println!("{}", note.text),
            NoteLevel::Warn => eprintln!("{}", note.text),
        }
    }
}
