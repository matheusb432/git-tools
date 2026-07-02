//! The `diffs` endpoints and the one place mapping wire DTOs ↔ domain/application
//! types (template pattern: `endpoints/todos/mod.rs`).

pub mod render;

use application::{
    diffs::render_diff::{RenderDiff, RenderDiffOutcome, RenderDiffResponse},
    shared::notes as app_notes,
};
use contracts::{
    diffs::{DiffTargetDto, RenderDiffData, RenderDiffRequest},
    envelope::{Envelope, Note, NoteLevel, Outcome},
};
use domain::diffs::DiffTarget;

/// Map the wire request DTO onto the application request.
///
/// # Errors
/// Returns an error when the DTO carries an invalid selection (e.g. a `last` count of zero).
pub(crate) fn to_request(dto: RenderDiffRequest) -> anyhow::Result<RenderDiff> {
    let target = match dto.target {
        DiffTargetDto::Unpushed => DiffTarget::Unpushed,
        DiffTargetDto::Base { rev } => DiffTarget::Base(rev),
        DiffTargetDto::Range { range } => DiffTarget::Range(range),
        DiffTargetDto::Merge { base } => DiffTarget::Merge(base),
        DiffTargetDto::Last { count } => DiffTarget::Last(
            std::num::NonZeroU32::new(count)
                .ok_or_else(|| anyhow::anyhow!("last count must be >= 1"))?,
        ),
    };
    Ok(RenderDiff {
        cwd: dto.cwd.into(),
        store_root: dto.store_root.into(),
        target,
        name: dto.name,
        theme: dto.theme,
    })
}

/// Project a successful application response onto the wire envelope.
pub(crate) fn to_envelope(resp: RenderDiffResponse) -> Envelope<RenderDiffData> {
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

/// Build an error envelope carrying a single `Error`-level note.
pub(crate) fn error_envelope(text: String) -> Envelope<RenderDiffData> {
    Envelope {
        outcome: Outcome::Error,
        notes: vec![Note {
            level: NoteLevel::Error,
            text,
        }],
        data: None,
    }
}

/// Map an application note (Info/Warn only) onto its wire counterpart.
fn to_note(n: &app_notes::Note) -> Note {
    Note {
        level: match n.level {
            app_notes::NoteLevel::Info => NoteLevel::Info,
            app_notes::NoteLevel::Warn => NoteLevel::Warn,
        },
        text: n.text.clone(),
    }
}
