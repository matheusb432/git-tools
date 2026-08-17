use anyhow::Context as _;
use gtl_application::{
    ports::{
        DiffRenderOutcome, DiffRenderRequest, DiffRenderResponse, DiffViewerClient, PlacedArtifact,
    },
    shared::notes::{Note, NoteLevel},
};
use gtl_wire::{
    envelope::{Envelope, Outcome},
    recipes::OpenRecipes,
};

use crate::client::HttpClient;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CliDiffViewerClient;

impl DiffViewerClient for CliDiffViewerClient {
    fn forward(&self, batch: &OpenRecipes) -> anyhow::Result<()> {
        let bin = crate::viewer::resolve_viewer_bin()
            .context("gtl-viewer is not installed; cannot forward the recipe batch")?;
        let token = gtl_wire::recipes::encode_token(batch)
            .context("failed to encode the viewer recipe batch")?;
        gtl_infra::detached_process::spawn(&bin, &[token.as_str()])
            .context("failed to spawn gtl-viewer to forward the recipe batch")
    }

    fn render(&self, request: &DiffRenderRequest) -> anyhow::Result<DiffRenderResponse> {
        let client = HttpClient::ensure_daemon()?;
        let envelope = match request {
            DiffRenderRequest::Diff(request) => client.render_diff(request)?,
            DiffRenderRequest::MergeDiff(request) => client.render_merge_diff(request)?,
            DiffRenderRequest::Subrepos(request) => client.render_diff_subrepos(request)?,
            DiffRenderRequest::Projects(request) => client.render_diff_all(request)?,
        };
        from_wire_response(envelope)
    }
}

fn from_wire_response(
    envelope: Envelope<gtl_wire::diffs::RenderDiffData>,
) -> anyhow::Result<DiffRenderResponse> {
    let notes = envelope
        .notes
        .iter()
        .filter_map(|note| match note.level {
            gtl_wire::envelope::NoteLevel::Info => Some(Note::info(note.text.clone())),
            gtl_wire::envelope::NoteLevel::Warn => Some(Note::warn(note.text.clone())),
            gtl_wire::envelope::NoteLevel::Error => None,
        })
        .collect();
    let outcome = match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            DiffRenderOutcome::Rendered(match data {
                gtl_wire::diffs::RenderDiffData::Created { artifact } => {
                    PlacedArtifact::Created { path: artifact }
                }
                gtl_wire::diffs::RenderDiffData::Reused { artifact } => {
                    PlacedArtifact::Reused { path: artifact }
                }
            })
        }
        Outcome::Empty => DiffRenderOutcome::Empty,
        Outcome::Error => anyhow::bail!("{}", crate::commands::error_text(&envelope.notes)),
    };
    Ok(DiffRenderResponse { outcome, notes })
}

pub(crate) fn print_notes(notes: &[Note]) {
    for note in notes {
        match note.level {
            NoteLevel::Info => println!("{}", note.text),
            NoteLevel::Warn => eprintln!("{}", note.text),
        }
    }
}
