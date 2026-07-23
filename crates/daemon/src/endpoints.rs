//! The daemon's HTTP endpoints, one module per route.

pub mod diffs;
mod error;
pub mod health;
pub mod live_views;
pub mod managed;
pub mod shutdown;

use contracts::envelope::{Envelope, Note, NoteLevel, Outcome};
pub(crate) use error::EndpointError;

/// Build an error envelope carrying a single `Error`-level note.
pub(crate) fn error_envelope<D>(text: String) -> Envelope<D> {
    Envelope {
        outcome: Outcome::Error,
        notes: vec![Note {
            level: NoteLevel::Error,
            text,
        }],
        data: None,
    }
}
