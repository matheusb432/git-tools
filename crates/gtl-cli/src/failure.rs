//! Renders command failures and derives their exit status.

use std::error::Error;

use gtl_client::RequestFailure;
use gtl_models::failure::{ErrorClass, Failure};

use crate::ExitCode;

/// The server declined the operation in its current state and explained why in band.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct Refusal(pub(crate) String);

/// What a failed command prints to stderr and how the process exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandFailure {
    message: String,
    diagnostic: Option<String>,
    exit: ExitCode,
}

impl CommandFailure {
    /// Decodes the first typed failure in the error chain; other errors fail with their text.
    pub(crate) fn from_error(error: &anyhow::Error) -> Self {
        for source in error.chain() {
            if let Some(failure) = source.downcast_ref::<gtl_client::ClientError>() {
                return Self::from_request(failure.failure(), failure.status().message());
            }
            if let Some(failure) = source.downcast_ref::<gtl_client::ConnectError>() {
                let detail = match failure {
                    gtl_client::ConnectError::Transport(transport) => chain_text(transport),
                    gtl_client::ConnectError::Health(status) => status.message().to_owned(),
                };
                return Self::from_request(failure.failure(), &detail);
            }
            if let Some((server_info, failure)) = source
                .downcast_ref::<gtl_client::ViewerServerInfoError>()
                .and_then(|error| error.failure().map(|failure| (error, failure)))
            {
                return Self::from_request(failure, &server_info.to_string());
            }
            if let Some(Refusal(detail)) = source.downcast_ref::<Refusal>() {
                return Self {
                    message: detail.clone(),
                    diagnostic: None,
                    exit: ExitCode::Refused,
                };
            }
        }
        Self {
            message: format!("{error:#}"),
            diagnostic: None,
            exit: ExitCode::Failed,
        }
    }

    /// Presents a request failure; `transport_detail` explains a lost connection.
    fn from_request(failure: RequestFailure, transport_detail: &str) -> Self {
        match failure {
            RequestFailure::Failed(failure) => Self::from_failure(&failure),
            RequestFailure::Disconnected => Self {
                message: "Could not reach the local gtl-server.".to_owned(),
                diagnostic: Some(transport_detail.to_owned()),
                exit: ExitCode::Unavailable,
            },
        }
    }

    fn from_failure(failure: &Failure) -> Self {
        Self {
            message: failure.to_string(),
            diagnostic: failure
                .diagnostic()
                .map(|diagnostic| diagnostic.as_str().to_owned()),
            exit: failure.class().into(),
        }
    }

    pub(crate) const fn exit(&self) -> ExitCode {
        self.exit
    }

    /// Renders the message after an optional command prefix, with the diagnostic indented below.
    pub(crate) fn text(&self, prefix: Option<&str>) -> String {
        let mut text = prefix.map_or_else(
            || self.message.clone(),
            |prefix| format!("{prefix}: {}", self.message),
        );
        for line in self
            .diagnostic
            .iter()
            .flat_map(|diagnostic| diagnostic.lines())
        {
            text.push('\n');
            if !line.is_empty() {
                text.push_str("  ");
                text.push_str(line);
            }
        }
        text
    }

    /// Prints the failure to stderr and returns its exit status.
    pub(crate) fn report(&self, prefix: Option<&str>) -> ExitCode {
        eprintln!("{}", self.text(prefix));
        self.exit
    }
}

/// Reports `error` under a command prefix and returns its exit status.
pub(crate) fn fail(prefix: &str, error: &anyhow::Error) -> ExitCode {
    CommandFailure::from_error(error).report(Some(prefix))
}

impl From<ErrorClass> for ExitCode {
    fn from(class: ErrorClass) -> Self {
        match class {
            ErrorClass::InvalidArgument | ErrorClass::OutOfRange => Self::Usage,
            ErrorClass::FailedPrecondition
            | ErrorClass::NotFound
            | ErrorClass::AlreadyExists
            | ErrorClass::Aborted
            | ErrorClass::PermissionDenied
            | ErrorClass::Unauthenticated => Self::Refused,
            ErrorClass::Unavailable
            | ErrorClass::ResourceExhausted
            | ErrorClass::DeadlineExceeded
            | ErrorClass::Cancelled => Self::Unavailable,
            ErrorClass::Internal | ErrorClass::DataLoss | ErrorClass::Unimplemented => Self::Failed,
        }
    }
}

fn chain_text(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use gtl_models::failure::{ExternalDiagnostic, PushFailure, Resource, SettingsFailure};
    use gtl_wire::proto::failure::encode_status;

    use super::*;

    fn rpc_error(failure: &Failure) -> anyhow::Error {
        gtl_client::ClientError::from(encode_status(failure.class(), failure)).into()
    }

    #[test]
    fn typed_failures_render_their_message_and_indented_diagnostic() {
        let failure =
            CommandFailure::from_error(&rpc_error(&Failure::Settings(SettingsFailure::Invalid {
                path: "/home/user/.config/git-tools/config.toml".into(),
                diagnostic: ExternalDiagnostic::new("expected `=`\n\nat line 2"),
            })));

        assert_eq!(
            failure.text(Some("push")),
            "push: User settings at /home/user/.config/git-tools/config.toml are invalid. \
             Repair the settings file or back it up and reset it.\n  expected `=`\n\n  at line 2"
        );
        assert_eq!(failure.exit(), ExitCode::Refused);
    }

    #[test]
    fn exit_status_follows_the_failure_class() {
        for (failure, exit) in [
            (Failure::Unexpected, ExitCode::Failed),
            (
                Failure::InvalidRequest {
                    field: "repository_path".into(),
                },
                ExitCode::Usage,
            ),
            (Failure::Push(PushFailure::NothingToPush), ExitCode::Refused),
            (
                Failure::Gone {
                    resource: Resource::Project,
                },
                ExitCode::Refused,
            ),
            (Failure::Busy, ExitCode::Unavailable),
        ] {
            assert_eq!(
                CommandFailure::from_error(&rpc_error(&failure)).exit(),
                exit
            );
        }
    }

    #[test]
    fn lost_connections_are_unavailable_and_keep_the_transport_text() {
        let error: anyhow::Error =
            gtl_client::ClientError::from(tonic::Status::unavailable("connection refused")).into();

        let failure = CommandFailure::from_error(&error);

        assert_eq!(
            failure.text(Some("pull")),
            "pull: Could not reach the local gtl-server.\n  connection refused"
        );
        assert_eq!(failure.exit(), ExitCode::Unavailable);
    }

    #[test]
    fn refusals_exit_as_refused_and_other_errors_keep_their_text() {
        let refusal = CommandFailure::from_error(&anyhow::Error::new(Refusal(
            "Check out a branch before pushing".into(),
        )));
        let other = CommandFailure::from_error(&anyhow::anyhow!("fatal: bad ref\nnot a commit"));

        assert_eq!(refusal.exit(), ExitCode::Refused);
        assert_eq!(
            refusal.text(Some("push")),
            "push: Check out a branch before pushing"
        );
        assert_eq!(other.exit(), ExitCode::Failed);
        assert_eq!(other.text(None), "fatal: bad ref\nnot a commit");
    }
}
