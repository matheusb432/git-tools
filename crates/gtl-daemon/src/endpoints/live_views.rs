//! `/live-views/*` endpoints: wire mapping between the live-views contracts
//! and the application slices.

pub mod save;

use gtl_application::{
    live_views::save_live_view::{SaveLiveView, SaveLiveViewOk, SaveLiveViewOutcome},
    shared::notes,
};
use gtl_wire::{
    envelope::{Envelope, Note, NoteLevel, Outcome},
    live_views::{SaveLiveViewData, SaveLiveViewDisposition, SaveLiveViewRequest},
};

/// Map the wire request into the application command.
pub(crate) fn to_request(dto: SaveLiveViewRequest) -> SaveLiveView {
    SaveLiveView {
        path: dto.path.into(),
    }
}

/// Map the slice response into the wire envelope. A `Rejected` outcome needs no
/// extra mapping — the rejection message already rides in `resp.notes`.
pub(crate) fn to_envelope(resp: SaveLiveViewOk) -> Envelope<SaveLiveViewData> {
    let notes = resp.notes.iter().map(to_note).collect();
    match resp.outcome {
        SaveLiveViewOutcome::Created { record } => {
            saved_envelope(notes, record, SaveLiveViewDisposition::Created)
        }
        SaveLiveViewOutcome::Refreshed { record } => {
            saved_envelope(notes, record, SaveLiveViewDisposition::Refreshed)
        }
        SaveLiveViewOutcome::Rejected { .. } => Envelope {
            outcome: Outcome::Error,
            notes,
            data: None,
        },
    }
}

fn saved_envelope(
    notes: Vec<Note>,
    record: gtl_application::live_views::LiveViewRecord,
    disposition: SaveLiveViewDisposition,
) -> Envelope<SaveLiveViewData> {
    Envelope {
        outcome: Outcome::Ok,
        notes,
        data: Some(SaveLiveViewData {
            source: record.source,
            display_name: record.display_name,
            disposition,
        }),
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
