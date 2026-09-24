mod diff;
mod live_view;
mod project;
mod repository;
mod settings;
mod status;
mod tag;
mod viewer;

use std::path::PathBuf;

pub(crate) use diff::DiffGrpcService;
use gtl_application::{ports::PlacedArtifact, shared::notes};
use gtl_models::{
    failure::{ErrorClass, Failure},
    paths::RepositoryRoot,
};
use gtl_wire::{proto::failure::encode_status, v1};
pub(crate) use live_view::LiveViewGrpcService;
pub(crate) use project::ProjectGrpcService;
pub(crate) use repository::RepositoryGrpcService;
pub(crate) use settings::SettingsGrpcService;
use status::invalid_request;
pub(crate) use tag::TagGrpcService;
use tonic::Status;
pub(crate) use viewer::{ViewerGrpcService, ViewerServerInfo};

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
        Err(invalid_request(field))
    }
}

pub(crate) fn repository_root(raw: String, field: &'static str) -> Result<RepositoryRoot, Status> {
    RepositoryRoot::try_new(PathBuf::from(raw)).map_err(|_| invalid_request(field))
}

pub(crate) fn required<T>(value: Option<T>, field: &'static str) -> Result<T, Status> {
    value.ok_or_else(|| invalid_request(field))
}

pub(crate) async fn run_blocking<T>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Status>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| unexpected(error, "run blocking task"))
}

/// Logs an unclassified failure and reports only a generic reason to the caller.
pub(crate) fn unexpected(error: impl std::fmt::Debug, operation: &'static str) -> Status {
    tracing::error!(error = ?error, operation, "gRPC application operation failed");
    encode_status(ErrorClass::Internal, &Failure::Unexpected)
}

#[cfg(test)]
mod tests {
    use std::io;

    use gtl_application::ports::{
        ProjectCatalogueDataError, ProjectCatalogueUnavailableError, ProjectClientError,
    };
    use gtl_models::failure::Failure;
    use gtl_wire::proto::failure::{StatusFailure, decode_status};
    use tonic::Code;

    use super::{status::status, unexpected};

    #[test]
    fn maps_project_catalogue_failures_by_caller_relevant_semantics() {
        let cases: [(ProjectClientError, Code, Failure); 2] = [
            (
                ProjectCatalogueUnavailableError::Dependency(anyhow::Error::new(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "private timeout",
                )))
                .into(),
                Code::Unavailable,
                Failure::Unavailable,
            ),
            (
                ProjectCatalogueDataError::Dependency(anyhow::Error::new(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "private record contents",
                )))
                .into(),
                Code::DataLoss,
                Failure::Unexpected,
            ),
        ];

        for (error, code, failure) in cases {
            let status = status(&error);

            assert_eq!(status.code(), code);
            assert_eq!(decode_status(&status), StatusFailure::Decoded(failure));
            assert!(!status.message().contains("private"));
        }
    }

    #[test]
    fn unexpected_application_failure_hides_its_display_message() {
        let status = unexpected(
            anyhow::anyhow!("read working tree: object database is unavailable"),
            "plan repository push",
        );

        assert_eq!(status.code(), Code::Internal);
        assert!(!status.message().contains("object database"));
        assert_eq!(
            decode_status(&status),
            StatusFailure::Decoded(Failure::Unexpected)
        );
    }
}
