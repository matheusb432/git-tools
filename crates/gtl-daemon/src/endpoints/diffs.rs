//! Diff endpoint response projection onto the shared wire envelope.

pub mod all;
pub mod merge;
pub mod render;
pub mod subrepos;

use gtl_application::{
    diffs::{
        render_diff::{RenderDiffOk, RenderDiffOutcome},
        render_diff_subrepos::{RenderDiffSubreposOk, RenderDiffSubreposOutcome},
        render_merge_diff::RenderMergeDiffOk,
    },
    ports::PlacedArtifact,
    projects::render_project_diff::RenderProjectDiffOk,
    shared::notes,
};
use gtl_wire::{
    diffs::RenderDiffData,
    envelope::{Envelope, Note, NoteLevel, Outcome},
};

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_envelope(resp: RenderDiffOk) -> Envelope<RenderDiffData> {
    let notes = resp.notes.iter().map(to_note).collect();
    match resp.outcome {
        RenderDiffOutcome::Rendered(placement) => Envelope {
            outcome: Outcome::Ok,
            notes,
            data: Some(to_render_data(&placement)),
        },
        RenderDiffOutcome::Empty => Envelope {
            outcome: Outcome::Empty,
            notes,
            data: None,
        },
    }
}

/// Shared by every response containing one placed artifact and notes.
fn ok_envelope(placement: &PlacedArtifact, notes: &[notes::Note]) -> Envelope<RenderDiffData> {
    Envelope {
        outcome: Outcome::Ok,
        notes: notes.iter().map(to_note).collect(),
        data: Some(to_render_data(placement)),
    }
}

fn to_render_data(placement: &PlacedArtifact) -> RenderDiffData {
    match placement {
        PlacedArtifact::Created { path } => RenderDiffData::Created {
            artifact: path.clone(),
        },
        PlacedArtifact::Reused { path } => RenderDiffData::Reused {
            artifact: path.clone(),
        },
    }
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_merge_envelope(resp: &RenderMergeDiffOk) -> Envelope<RenderDiffData> {
    ok_envelope(&resp.placement, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_all_envelope(resp: &RenderProjectDiffOk) -> Envelope<RenderDiffData> {
    ok_envelope(&resp.placement, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_subrepos_envelope(resp: RenderDiffSubreposOk) -> Envelope<RenderDiffData> {
    let notes = resp.notes.iter().map(to_note).collect();
    match resp.outcome {
        RenderDiffSubreposOutcome::Rendered(placement) => Envelope {
            outcome: Outcome::Ok,
            notes,
            data: Some(to_render_data(&placement)),
        },
        RenderDiffSubreposOutcome::Empty => Envelope {
            outcome: Outcome::Empty,
            notes,
            data: None,
        },
    }
}

/// Map an application note (Info/Warn only) onto its wire counterpart.
fn to_note(n: &notes::Note) -> Note {
    Note {
        level: match n.level {
            notes::NoteLevel::Info => NoteLevel::Info,
            notes::NoteLevel::Warn => NoteLevel::Warn,
        },
        text: n.text.clone(),
    }
}
