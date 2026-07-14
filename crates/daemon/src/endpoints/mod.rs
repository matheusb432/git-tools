//! The daemon's HTTP endpoints, one module per route.

pub mod diffs;
pub mod health;
pub mod live_views;
pub mod managed;
pub mod shutdown;

use std::sync::Arc;

use axum::{Json, http::StatusCode};
use contracts::envelope::{Envelope, Note, NoteLevel, Outcome};

use crate::state::Shared;

/// Shared endpoint body for every request handled off the blocking pool: touch,
/// map request errors to 400, run the operation away from Tokio workers, then
/// project the response or map operation failures to 500.
pub(crate) async fn run<R, O, E, D>(
    shared: Arc<Shared>,
    req: anyhow::Result<R>,
    execute: impl FnOnce(R) -> Result<O, E> + Send + 'static,
    project: impl FnOnce(O) -> Envelope<D> + Send + 'static,
) -> (StatusCode, Json<Envelope<D>>)
where
    R: Send + 'static,
    O: Send + 'static,
    E: std::fmt::Display + std::error::Error + Send + Sync + 'static,
    D: Send + 'static,
{
    shared.touch();
    let req = match req {
        Ok(req) => req,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(error_envelope(format!("{e:#}"))),
            );
        }
    };
    // The operation does synchronous, potentially slow I/O (git subprocess, SQLite) — run
    // it on the blocking pool.
    let joined = tokio::task::spawn_blocking(move || execute(req)).await;
    match joined {
        Ok(Ok(resp)) => (StatusCode::OK, Json(project(resp))),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_envelope(format!("{e:#}"))),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(error_envelope(format!("daemon task panicked: {e}"))),
        ),
    }
}

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
