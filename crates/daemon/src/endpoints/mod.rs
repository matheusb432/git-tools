//! The daemon's HTTP endpoints, one module per route.

pub mod diffs;
pub mod health;
pub mod live_views;
pub mod managed;
pub mod shutdown;

use std::sync::Arc;

use axum::{Json, http::StatusCode};
use contracts::envelope::{Envelope, Note, NoteLevel, Outcome};
use cqrsy::{Handle, Request, Sender};

use crate::state::Shared;

/// Shared endpoint body for every request handled off the blocking pool: touch,
/// map-error → 400, run the handler via `send_now` (the handler shells out or
/// hits `SQLite` synchronously), then project the response (or 500 on failure).
pub(crate) async fn run<H, R, D>(
    handler: H,
    shared: Arc<Shared>,
    req: anyhow::Result<R>,
    project: impl FnOnce(R::Output) -> Envelope<D> + Send + 'static,
) -> (StatusCode, Json<Envelope<D>>)
where
    H: Sender<R> + Handle,
    R: Request + Send + 'static,
    R::Output: Send + 'static,
    R::Error: std::fmt::Display + Send + 'static,
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
    // The handler does synchronous, potentially slow I/O (git subprocess, SQLite) — run
    // it on the blocking pool.
    let joined = tokio::task::spawn_blocking(move || handler.send_now(req)).await;
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
