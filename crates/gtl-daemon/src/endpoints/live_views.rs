//! `/live-views/*` endpoints: wire mapping between the live-views contracts
//! and the application slices.

pub mod save;

use gtl_application::{
    live_views::save::{SaveLiveView, SaveLiveViewOk, SaveLiveViewOutcome},
    shared::notes,
};
use gtl_contracts::{
    envelope::{Envelope, Note, NoteLevel, Outcome},
    live_views::{SaveLiveViewData, SaveLiveViewRequest},
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
        SaveLiveViewOutcome::Saved {
            record,
            already_saved,
        } => Envelope {
            outcome: Outcome::Ok,
            notes,
            data: Some(SaveLiveViewData {
                source_kind: record.source_kind,
                source_value: record.source_value,
                display_name: record.display_name,
                already_saved,
            }),
        },
        SaveLiveViewOutcome::Rejected { .. } => Envelope {
            outcome: Outcome::Error,
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
