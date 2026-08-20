mod diff;
mod live_view;
mod project;
mod repository;
mod settings;
mod tag;
mod worktree;

use std::path::PathBuf;

pub(crate) use diff::DiffApi;
use gtl_application::{
    ports::{PlacedArtifact, ProjectClientError},
    shared::notes,
};
use gtl_models::paths::RepositoryRoot;
use gtl_wire::v1;
pub(crate) use live_view::LiveViewApi;
pub(crate) use project::ProjectApi;
pub(crate) use repository::RepositoryApi;
pub(crate) use settings::SettingsApi;
pub(crate) use tag::TagApi;
use tonic::Status;
pub(crate) use worktree::WorktreeApi;

pub(crate) fn application_notes(notes: &[notes::Note]) -> Vec<v1::Note> {
    notes
        .iter()
        .map(|note| v1::Note {
            level: match note.level {
                notes::NoteLevel::Info => v1::NoteLevel::Info as i32,
                notes::NoteLevel::Warn => v1::NoteLevel::Warning as i32,
            },
            text: note.text.clone(),
        })
        .collect()
}

pub(crate) fn artifact(placed: &PlacedArtifact) -> v1::Artifact {
    let (path, placement) = match placed {
        PlacedArtifact::Created { path } => (path, v1::ArtifactPlacement::Created),
        PlacedArtifact::Reused { path } => (path, v1::ArtifactPlacement::Reused),
    };
    v1::Artifact {
        path: path.to_string_lossy().into_owned(),
        placement: placement as i32,
    }
}

pub(crate) fn absolute_path(raw: String, field: &'static str) -> Result<PathBuf, Status> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(Status::invalid_argument(format!(
            "{field} must be an absolute path"
        )))
    }
}

pub(crate) fn repository_root(raw: String, field: &'static str) -> Result<RepositoryRoot, Status> {
    RepositoryRoot::try_new(PathBuf::from(raw))
        .map_err(|_| Status::invalid_argument(format!("{field} must be an absolute path")))
}

pub(crate) fn required<T>(value: Option<T>, field: &'static str) -> Result<T, Status> {
    value.ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

pub(crate) fn project_client_error(error: &ProjectClientError) -> Status {
    match error {
        ProjectClientError::Unavailable { .. } => {
            tracing::warn!(error = ?error, "project catalogue is unavailable");
            Status::unavailable("project catalogue is unavailable")
        }
        ProjectClientError::InvalidData { .. } => {
            tracing::error!(error = ?error, "project catalogue returned invalid data");
            Status::data_loss("project catalogue returned invalid data")
        }
    }
}

pub(crate) fn task_join(error: &tokio::task::JoinError) -> Status {
    tracing::error!(error = ?error, "gRPC blocking task failed");
    Status::internal("server operation failed")
}

pub(crate) fn unexpected(error: impl std::fmt::Debug, operation: &'static str) -> Status {
    tracing::error!(error = ?error, operation, "gRPC application operation failed");
    Status::internal(format!("{operation} failed"))
}
