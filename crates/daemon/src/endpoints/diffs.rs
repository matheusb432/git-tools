//! Diff endpoint response projection onto the shared wire envelope.

pub mod all;
pub mod merge;
pub mod render;
pub mod squash_preview;
pub mod subrepos;

use application::{
    diffs::{
        render_diff::{RenderDiffOk, RenderDiffOutcome},
        render_diff_all::RenderDiffAllOk,
        render_diff_subrepos::{RenderDiffSubreposOk, RenderDiffSubreposOutcome},
        render_merge_diff::RenderMergeDiffOk,
        render_squash_preview::RenderSquashPreviewOk,
    },
    shared::notes,
};
use contracts::{
    diffs::RenderDiffData,
    envelope::{Envelope, Note, NoteLevel, Outcome},
};

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_envelope(resp: RenderDiffOk) -> Envelope<RenderDiffData> {
    let notes = resp.notes.iter().map(to_note).collect();
    match resp.outcome {
        RenderDiffOutcome::Rendered { artifact, reused } => Envelope {
            outcome: Outcome::Ok,
            notes,
            data: Some(RenderDiffData {
                artifact: artifact.to_string_lossy().into_owned(),
                reused,
            }),
        },
        RenderDiffOutcome::Empty => Envelope {
            outcome: Outcome::Empty,
            notes,
            data: None,
        },
    }
}

/// Shared by every response shaped `{artifact, reused, notes}`.
fn ok_envelope(
    artifact: &std::path::Path,
    reused: bool,
    notes: &[notes::Note],
) -> Envelope<RenderDiffData> {
    Envelope {
        outcome: Outcome::Ok,
        notes: notes.iter().map(to_note).collect(),
        data: Some(RenderDiffData {
            artifact: artifact.to_string_lossy().into_owned(),
            reused,
        }),
    }
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_merge_envelope(resp: &RenderMergeDiffOk) -> Envelope<RenderDiffData> {
    ok_envelope(&resp.artifact, resp.reused, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_squash_envelope(resp: &RenderSquashPreviewOk) -> Envelope<RenderDiffData> {
    ok_envelope(&resp.artifact, resp.reused, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_all_envelope(resp: &RenderDiffAllOk) -> Envelope<RenderDiffData> {
    ok_envelope(&resp.artifact, resp.reused, &resp.notes)
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_subrepos_envelope(resp: RenderDiffSubreposOk) -> Envelope<RenderDiffData> {
    let notes = resp.notes.iter().map(to_note).collect();
    match resp.outcome {
        RenderDiffSubreposOutcome::Rendered { artifact, reused } => Envelope {
            outcome: Outcome::Ok,
            notes,
            data: Some(RenderDiffData {
                artifact: artifact.to_string_lossy().into_owned(),
                reused,
            }),
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
