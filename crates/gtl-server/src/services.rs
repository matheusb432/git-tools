mod diff;
mod live_view;
mod project;
mod repository;
mod settings;
mod tag;
mod viewer;
mod worktree;

use std::path::PathBuf;

pub(crate) use diff::DiffGrpcService;
use gtl_application::{
    ports::{
        PlacedArtifact, ProjectClientError, UserSettingsConfigurationError, UserSettingsLoadError,
    },
    repositories::resolve_repository_root::ResolveRepositoryRootError,
    shared::notes,
};
use gtl_models::paths::RepositoryRoot;
use gtl_wire::v1;
pub(crate) use live_view::LiveViewGrpcService;
pub(crate) use project::ProjectGrpcService;
pub(crate) use repository::RepositoryGrpcService;
pub(crate) use settings::SettingsGrpcService;
pub(crate) use tag::TagGrpcService;
use tonic::{Code, Status};
pub(crate) use viewer::ViewerGrpcService;
pub(crate) use worktree::WorktreeGrpcService;

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
        ProjectClientError::InvalidConfiguration(_) => {
            project_client_warning(error, Code::FailedPrecondition)
        }
        ProjectClientError::Unavailable(_) => project_client_warning(error, Code::Unavailable),
        ProjectClientError::InvalidData(_) => project_client_failure(error, Code::DataLoss),
    }
}

fn project_client_warning(error: &ProjectClientError, code: Code) -> Status {
    tracing::warn!(error = ?error, grpc_code = ?code, "project catalogue request failed");
    Status::new(code, error.to_string())
}

fn project_client_failure(error: &ProjectClientError, code: Code) -> Status {
    tracing::error!(error = ?error, grpc_code = ?code, "project catalogue request failed");
    Status::new(code, error.to_string())
}

pub(crate) fn user_settings_load_error(error: UserSettingsLoadError) -> Status {
    match error {
        UserSettingsLoadError::InvalidConfiguration(error) => {
            invalid_user_settings_configuration(&error, "load user settings")
        }
        error @ UserSettingsLoadError::Adapter(_) => unexpected(error, "load user settings"),
    }
}

pub(crate) fn invalid_user_settings_configuration(
    error: &UserSettingsConfigurationError,
    operation: &'static str,
) -> Status {
    let message = error.client_message().to_owned();
    tracing::warn!(error = ?error, operation, "user settings are invalid");
    Status::failed_precondition(message)
}

pub(crate) fn repository_resolution_error(error: ResolveRepositoryRootError) -> Status {
    match error {
        ResolveRepositoryRootError::Rejected { detail, .. } => Status::failed_precondition(detail),
        error => unexpected(error, "resolve repository"),
    }
}

pub(crate) async fn run_blocking<T>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Status>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| {
            tracing::error!(error = ?error, "gRPC blocking task failed");
            Status::internal("server operation failed")
        })
}

pub(crate) fn unexpected(
    error: impl std::fmt::Debug + std::fmt::Display,
    operation: &'static str,
) -> Status {
    tracing::error!(error = ?error, operation, "gRPC application operation failed");
    Status::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::io;

    use gtl_application::ports::{
        ProjectCatalogueConfigurationError, ProjectCatalogueDataError,
        ProjectCatalogueUnavailableError, ProjectClientError,
    };
    use tonic::Code;

    use super::{project_client_error, unexpected};

    #[test]
    fn maps_project_catalogue_failures_by_caller_relevant_semantics() {
        let cases: [(ProjectClientError, Code, &str); 3] = [
            (
                ProjectCatalogueConfigurationError::HomeDirectoryUnavailable.into(),
                Code::FailedPrecondition,
                "project catalogue home directory is unavailable",
            ),
            (
                ProjectCatalogueUnavailableError::Dependency(anyhow::Error::new(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "private timeout",
                )))
                .into(),
                Code::Unavailable,
                "project catalogue is temporarily unavailable",
            ),
            (
                ProjectCatalogueDataError::Dependency(anyhow::Error::new(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "private record contents",
                )))
                .into(),
                Code::DataLoss,
                "project catalogue returned invalid data",
            ),
        ];

        for (error, code, message) in cases {
            let status = project_client_error(&error);

            assert_eq!(status.code(), code);
            assert_eq!(status.message(), message);
            assert!(!status.message().contains("private"));
        }
    }

    #[test]
    fn unexpected_application_failure_preserves_its_display_message() {
        let status = unexpected(
            anyhow::anyhow!("read working tree: object database is unavailable"),
            "plan repository push",
        );

        assert_eq!(status.code(), Code::Internal);
        assert_eq!(
            status.message(),
            "read working tree: object database is unavailable"
        );
    }
}
